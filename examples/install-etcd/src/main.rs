use anyhow::bail;
use rusible::{
    init_forest_logging,
    inventory::{Host, Inventory},
    meta::{
        field::Field,
        task::{
            command::CommandTask,
            copy::CopyTask,
            download::DownloadTask,
            file::{FileState, FileTask},
            shell::ShellTask,
            stat::StatTask,
            systemd::{SystemdState, SystemdTask},
            unarchive::UnarchiveTask,
            user::UserTask,
            wait_for::WaitForTask,
        },
    },
    runtime::Runnable as _,
    shell::{shell_quote, shell_quote_path},
    target::{Local, UploadOptions},
    vars::VarLookupError,
};
use std::{
    env::temp_dir,
    net::IpAddr,
    path::{Path, PathBuf},
};
use tracing::info;

const RUSIBLE_EXEC_BYTES: &[u8] = include_bytes!(env!("CARGO_BIN_FILE_RUSIBLE_EXEC"));

#[derive(Debug, Clone)]
struct EtcdHostSpec {
    inventory_name: String,
    member_name: String,
    peer_host: String,
    client_host: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_forest_logging("info");

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let inventory_path = manifest_dir.join("inventory.toml");
    let mut inventory = Inventory::from_toml_path(&inventory_path)
        .await?
        .filter_by_group("etcd");

    if inventory.is_empty() {
        bail!("inventory did not select any hosts from group `etcd`");
    }

    let etcd_version = inventory.get_var("etcd.version")?;
    let local_cert_dir =
        resolve_local_path(&manifest_dir, &inventory.get_var("etcd.local_cert_dir")?);
    inventory.set_var("etcd.local_cert_dir", local_cert_dir.display().to_string())?;
    let remote_ssl_dir = inventory.get_var("etcd.remote_ssl_dir")?;
    let remote_data_dir = inventory.get_var("etcd.remote_data_dir")?;
    let remote_service_path = inventory.get_var("etcd.remote_service_path")?;
    let cluster_state = inventory.get_var("etcd.cluster_state")?;
    let cluster_token = inventory.get_var("etcd.cluster_token")?;
    let local_download_dir = temp_dir().join("downloads");

    let host_specs = inventory
        .hosts()
        .iter()
        .map(host_spec_from_inventory_host)
        .collect::<anyhow::Result<Vec<_>>>()?;

    let initial_cluster = host_specs
        .iter()
        .map(|host| format!("{}=https://{}:2380", host.member_name, host.peer_host))
        .collect::<Vec<_>>()
        .join(",");
    inventory.set_var("etcd.initial_cluster", initial_cluster.clone())?;

    info!(
        inventory = %inventory_path.display(),
        selected_hosts = inventory.len(),
        version = %etcd_version,
        initial_cluster = %initial_cluster,
        "loaded etcd inventory"
    );

    let mut local = Local::new();
    local.init(RUSIBLE_EXEC_BYTES).await?;
    inventory.init(RUSIBLE_EXEC_BYTES).await?;

    prepare_local_certificates(&mut local, &local_cert_dir, &host_specs).await?;
    install_etcd_runtime(
        &mut local,
        &mut inventory,
        &local_download_dir,
        &etcd_version,
        &remote_ssl_dir,
        &remote_data_dir,
        &remote_service_path,
    )
    .await?;
    distribute_certificates(&inventory).await?;

    inventory
        .run(FileTask {
            name: "Render etcd systemd unit".into(),
            path: PathBuf::from(&remote_service_path).into(),
            state: FileState::File.into(),
            content: Field::tpl(include_str!("etcd.service.j2")),
            mode: "0644".into(),
            ..Default::default()
        })
        .await?;

    inventory
        .run(SystemdTask {
            name: "Restart etcd service".into(),
            unit: "etcd.service".into(),
            daemon_reload: true.into(),
            enabled: true.into(),
            state: SystemdState::Restarted.into(),
        })
        .await?;

    inventory
        .run(WaitForTask {
            name: "Wait for etcd client port".into(),
            host: "127.0.0.1".into(),
            port: 2379.into(),
            delay_secs: 5.into(),
            timeout_secs: 30.into(),
            connect_timeout_secs: 2.into(),
        })
        .await?;

    inventory
        .run(CommandTask {
            name: "Check etcd endpoint health".into(),
            argv: vec![
                "/usr/bin/env".to_string(),
                "ETCDCTL_API=3".to_string(),
                "/usr/local/bin/etcdctl".to_string(),
                "--endpoints=https://127.0.0.1:2379".to_string(),
                format!("--cacert={remote_ssl_dir}/ca.crt"),
                format!("--cert={remote_ssl_dir}/server.crt"),
                format!("--key={remote_ssl_dir}/server.key"),
                "endpoint".to_string(),
                "health".to_string(),
            ]
            .into_iter()
            .map(Into::into)
            .collect(),
            ..Default::default()
        })
        .await?;

    info!(cluster_state = %cluster_state, cluster_token = %cluster_token, "install-etcd example finished");

    Ok(())
}

async fn prepare_local_certificates(
    local: &mut Local, local_cert_dir: &Path, host_specs: &[EtcdHostSpec],
) -> anyhow::Result<()> {
    local
        .run(FileTask {
            name: "Ensure local certificate directory".into(),
            path: local_cert_dir.to_path_buf().into(),
            state: FileState::Directory.into(),
            mode: "0755".into(),
            ..Default::default()
        })
        .await?;

    let ca_cert_path = local_cert_dir.join("ca.crt");
    let ca_key_path = local_cert_dir.join("ca.key");
    let ca_stat = local
        .run(StatTask {
            name: "Inspect local CA certificate".into(),
            path: ca_cert_path.clone().into(),
        })
        .await?;

    if !ca_stat
        .result
        .details
        .as_ref()
        .is_some_and(|details| details.exists)
    {
        local
            .run(ShellTask {
                name: "Generate local CA certificate".into(),
                cmd: format!(
                    "set -eu; openssl genrsa -out {quoted_ca_key_path} 2048; openssl req -x509 \
                     -new -nodes -key {quoted_ca_key_path} -subj '/CN=etcd-ca' -days 3650 -out \
                     {quoted_ca_cert_path}",
                    quoted_ca_key_path = shell_quote_path(&ca_key_path)?,
                    quoted_ca_cert_path = shell_quote_path(&ca_cert_path)?,
                )
                .into(),
                creates: ca_cert_path.clone().into(),
                ..Default::default()
            })
            .await?;
    }

    for host in host_specs {
        let key_path = local_cert_dir.join(format!("{}.key", host.inventory_name));
        let csr_path = local_cert_dir.join(format!("{}.csr", host.inventory_name));
        let crt_path = local_cert_dir.join(format!("{}.crt", host.inventory_name));
        let csr_config_path = local_cert_dir.join(format!("{}.csr.conf", host.inventory_name));
        let san_ext_path = local_cert_dir.join(format!("{}.san.ext", host.inventory_name));

        local
            .run(FileTask {
                name: format!("Render CSR config for {}", host.inventory_name).into(),
                path: csr_config_path.clone().into(),
                state: FileState::File.into(),
                mode: "0644".into(),
                content: render_csr_config(&build_cert_sans(host)).into(),
                ..Default::default()
            })
            .await?;

        local
            .run(FileTask {
                name: format!("Render SAN extension for {}", host.inventory_name).into(),
                path: san_ext_path.clone().into(),
                state: FileState::File.into(),
                mode: "0644".into(),
                content: format!("subjectAltName={}\n", build_cert_sans(host)).into(),
                ..Default::default()
            })
            .await?;

        local
            .run(ShellTask {
                name: format!("Generate certificate for {}", host.inventory_name).into(),
                cmd: format!(
                    concat!(
                        "set -eu; ",
                        "openssl genrsa -out {quoted_key_path} 2048; ",
                        "openssl req -new -key {quoted_key_path} -subj {quoted_subject} ",
                        "-config {quoted_csr_config_path} -extensions v3_req ",
                        "-out {quoted_csr_path}; ",
                        "openssl x509 -req -in {quoted_csr_path} -CA {quoted_ca_cert_path} ",
                        "-CAkey {quoted_ca_key_path} -CAcreateserial -days 365 ",
                        "-out {quoted_crt_path} -extfile {quoted_san_ext_path}"
                    ),
                    quoted_key_path = shell_quote_path(&key_path)?,
                    quoted_subject = shell_quote(format!("/CN={}", host.member_name))?,
                    quoted_csr_config_path = shell_quote_path(&csr_config_path)?,
                    quoted_csr_path = shell_quote_path(&csr_path)?,
                    quoted_ca_cert_path = shell_quote_path(&ca_cert_path)?,
                    quoted_ca_key_path = shell_quote_path(&ca_key_path)?,
                    quoted_crt_path = shell_quote_path(&crt_path)?,
                    quoted_san_ext_path = shell_quote_path(&san_ext_path)?,
                )
                .into(),
                creates: crt_path.into(),
                ..Default::default()
            })
            .await?;
    }

    Ok(())
}

async fn install_etcd_runtime(
    local: &mut Local, inventory: &mut Inventory, local_download_dir: &Path, version: &str,
    remote_ssl_dir: &str, remote_data_dir: &str, remote_service_path: &str,
) -> anyhow::Result<()> {
    let local_archive_path = local_download_dir.join(format!("etcd-{version}-linux-amd64.tar.gz"));
    let archive_path = PathBuf::from(format!("/tmp/etcd-{version}-linux-amd64.tar.gz"));
    let extract_dir = PathBuf::from(format!("/tmp/etcd-{version}-linux-amd64"));

    local
        .run(FileTask {
            name: "Ensure local etcd download directory".into(),
            path: local_download_dir.to_path_buf().into(),
            state: FileState::Directory.into(),
            mode: "0755".into(),
            ..Default::default()
        })
        .await?;

    inventory
        .run(UserTask {
            name: "Ensure etcd service user".into(),
            username: "etcd".into(),
            system: true.into(),
            create_home: false.into(),
            shell: PathBuf::from("/usr/sbin/nologin").into(),
            ..Default::default()
        })
        .await?;

    for path in [remote_ssl_dir, remote_data_dir] {
        inventory
            .run(FileTask {
                name: format!("Ensure remote directory {path}").into(),
                path: PathBuf::from(path).into(),
                state: FileState::Directory.into(),
                owner: "etcd".into(),
                group: "etcd".into(),
                mode: "0755".into(),
                ..Default::default()
            })
            .await?;
    }

    local
        .run(DownloadTask {
            name: "Download etcd release archive".into(),
            url: Field::val(format!(
                "https://github.com/etcd-io/etcd/releases/download/{version}/etcd-{version}-linux-amd64.tar.gz"
            )
            .parse()?),
            dest: local_archive_path.clone().into(),
            force: false.into(),
            mode: "0644".into(),
            ..Default::default()
        })
        .await?;

    inventory
        .upload_file(&local_archive_path, &archive_path, UploadOptions::default())
        .await?;

    inventory
        .run(UnarchiveTask {
            name: "Extract etcd release archive".into(),
            src: archive_path.into(),
            dest: PathBuf::from("/tmp").into(),
            creates: extract_dir.join("etcd").into(),
        })
        .await?;

    for binary in ["etcd", "etcdctl"] {
        inventory
            .run(CopyTask {
                name: format!("Install {binary} binary").into(),
                src: extract_dir.join(binary).into(),
                dest: PathBuf::from(format!("/usr/local/bin/{binary}")).into(),
                mode: "0755".into(),
                ..Default::default()
            })
            .await?;
    }

    inventory
        .run(FileTask {
            name: "Ensure etcd service file exists".into(),
            path: PathBuf::from(remote_service_path).into(),
            state: FileState::Touch.into(),
            mode: "0644".into(),
            ..Default::default()
        })
        .await?;

    Ok(())
}

async fn distribute_certificates(inventory: &Inventory) -> anyhow::Result<()> {
    inventory
        .upload_file(
            Field::tpl("{{ etcd.local_cert_dir }}/ca.crt"),
            Field::tpl("{{ etcd.remote_ssl_dir }}/ca.crt"),
            UploadOptions {
                owner: Some("etcd".to_string()),
                group: Some("etcd".to_string()),
                mode: Some("0600".to_string()),
            },
        )
        .await?;

    inventory
        .upload_file(
            Field::tpl("{{ etcd.local_cert_dir }}/{{ rusible.host.name }}.crt"),
            Field::tpl("{{ etcd.remote_ssl_dir }}/server.crt"),
            UploadOptions {
                owner: Some("etcd".to_string()),
                group: Some("etcd".to_string()),
                mode: Some("0600".to_string()),
            },
        )
        .await?;

    inventory
        .upload_file(
            Field::tpl("{{ etcd.local_cert_dir }}/{{ rusible.host.name }}.key"),
            Field::tpl("{{ etcd.remote_ssl_dir }}/server.key"),
            UploadOptions {
                owner: Some("etcd".to_string()),
                group: Some("etcd".to_string()),
                mode: Some("0600".to_string()),
            },
        )
        .await?;

    Ok(())
}

fn host_spec_from_inventory_host(host: &Host) -> anyhow::Result<EtcdHostSpec> {
    let member_name = host.remote().get_var("etcd.name")?;
    let peer_host = match host.remote().get_var("etcd.peer_host") {
        Ok(value) => value,
        Err(VarLookupError::Missing { .. }) => host.remote().host.clone(),
        Err(error) => return Err(error.into()),
    };
    let client_host = match host.remote().get_var("etcd.client_host") {
        Ok(value) => value,
        Err(VarLookupError::Missing { .. }) => peer_host.clone(),
        Err(error) => return Err(error.into()),
    };

    Ok(EtcdHostSpec {
        inventory_name: host.name().to_string(),
        member_name,
        peer_host,
        client_host,
    })
}

fn build_cert_sans(host: &EtcdHostSpec) -> String {
    let mut entries = vec![
        format!("DNS:{}", host.member_name),
        "IP:127.0.0.1".to_string(),
    ];

    push_subject_alt_name(&mut entries, &host.peer_host);
    if host.client_host != host.peer_host {
        push_subject_alt_name(&mut entries, &host.client_host);
    }

    entries.join(",")
}

fn push_subject_alt_name(entries: &mut Vec<String>, value: &str) {
    if value.parse::<IpAddr>().is_ok() {
        entries.push(format!("IP:{value}"));
    } else {
        entries.push(format!("DNS:{value}"));
    }
}

fn render_csr_config(subject_alt_names: &str) -> String {
    format!(
        concat!(
            "[req]\n",
            "distinguished_name=req\n",
            "[v3_req]\n",
            "keyUsage=critical,digitalSignature,keyEncipherment\n",
            "extendedKeyUsage=serverAuth,clientAuth\n",
            "subjectAltName={}\n",
        ),
        subject_alt_names,
    )
}

fn resolve_local_path(base_dir: &Path, configured: &str) -> PathBuf {
    let path = PathBuf::from(configured);
    if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    }
}
