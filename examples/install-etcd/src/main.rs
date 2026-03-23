use anyhow::bail;
use rusible::{
    inventory::{Host, Inventory},
    meta::{
        CommandTask, CopyTask, DownloadTask, FileState, FileTask, ShellTask, StatTask,
        SystemdState, SystemdTask, TemplateTask, UnarchiveTask, UserTask, WaitForTask,
    },
    runtime::Runnable as _,
    target::Local,
    TemplatePath, UploadOptions,
};
use std::{
    env::temp_dir,
    net::IpAddr,
    path::{Path, PathBuf},
};
use tracing::info;
use tracing_subscriber::EnvFilter;

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
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,rusible=debug")),
        )
        .init();

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let inventory_path = manifest_dir.join("inventory.toml");
    let mut inventory = Inventory::from_toml_path(&inventory_path)
        .await?
        .filter_by_group("etcd");

    if inventory.is_empty() {
        bail!("inventory did not select any hosts from group `etcd`");
    }

    let etcd_version = inventory.get_var("etcd.version")?;
    let local_cert_dir = resolve_local_path(
        &manifest_dir,
        &inventory.get_var("etcd.local_cert_dir")?,
    );
    inventory.set_var(
        "etcd.local_cert_dir",
        local_cert_dir.display().to_string(),
    )?;
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
        .run(TemplateTask {
            dest: PathBuf::from(&remote_service_path),
            content: include_str!("etcd.service.j2").to_string(),
            owner: None,
            group: None,
            mode: Some("0644".to_string()),
        })
        .await?;

    inventory
        .run(SystemdTask {
            unit: "etcd.service".to_string(),
            daemon_reload: true,
            enabled: Some(true),
            state: Some(SystemdState::Restarted),
        })
        .await?;

    inventory
        .run(WaitForTask {
            host: Some("127.0.0.1".to_string()),
            port: 2379,
            delay_secs: 5,
            timeout_secs: 30,
            connect_timeout_secs: 2,
        })
        .await?;

    let health_report = inventory
        .run(CommandTask {
            cmd: None,
            argv: Some(vec![
                "/usr/bin/env".to_string(),
                "ETCDCTL_API=3".to_string(),
                "/usr/local/bin/etcdctl".to_string(),
                "--endpoints=https://127.0.0.1:2379".to_string(),
                format!("--cacert={remote_ssl_dir}/ca.crt"),
                format!("--cert={remote_ssl_dir}/server.crt"),
                format!("--key={remote_ssl_dir}/server.key"),
                "endpoint".to_string(),
                "health".to_string(),
            ]),
            chdir: None,
            creates: None,
            removes: None,
            stdin: None,
        })
        .await?;

    for result in health_report.0 {
        let stdout = result
            .result
            .details
            .as_ref()
            .map(|details| details.stdout.trim())
            .unwrap_or("");
        let stderr = result
            .result
            .details
            .as_ref()
            .map(|details| details.stderr.trim())
            .unwrap_or("");

        info!(
            host = %result.host,
            exec_path = %result.exec_path,
            status = ?result.result.status,
            stdout,
            stderr,
            "etcd endpoint health completed"
        );
    }

    info!(cluster_state = %cluster_state, cluster_token = %cluster_token, "install-etcd example finished");

    Ok(())
}

async fn prepare_local_certificates(
    local: &mut Local,
    local_cert_dir: &Path,
    host_specs: &[EtcdHostSpec],
) -> anyhow::Result<()> {
    local
        .run(FileTask {
            path: local_cert_dir.to_path_buf(),
            state: FileState::Directory,
            owner: None,
            group: None,
            mode: Some("0755".to_string()),
            content: None,
        })
        .await?;

    let ca_cert_path = local_cert_dir.join("ca.crt");
    let ca_key_path = local_cert_dir.join("ca.key");
    let ca_stat = local
        .run(StatTask {
            path: ca_cert_path.clone(),
        })
        .await?;

    if !ca_stat.result.details.as_ref().is_some_and(|details| details.exists) {
        local
            .run(ShellTask {
                cmd: format!(
                    "set -eu; openssl genrsa -out {} 2048; openssl req -x509 -new -nodes -key {} -subj '/CN=etcd-ca' -days 3650 -out {}",
                    shell_quote_path(&ca_key_path),
                    shell_quote_path(&ca_key_path),
                    shell_quote_path(&ca_cert_path),
                ),
                chdir: None,
                creates: Some(ca_cert_path.clone()),
                removes: None,
                stdin: None,
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
                path: csr_config_path.clone(),
                state: FileState::File,
                owner: None,
                group: None,
                mode: Some("0644".to_string()),
                content: Some(render_csr_config(&build_cert_sans(host))),
            })
            .await?;

        local
            .run(FileTask {
                path: san_ext_path.clone(),
                state: FileState::File,
                owner: None,
                group: None,
                mode: Some("0644".to_string()),
                content: Some(format!("subjectAltName={}\n", build_cert_sans(host))),
            })
            .await?;

        local
            .run(ShellTask {
                cmd: format!(
                    concat!(
                        "set -eu; ",
                        "openssl genrsa -out {} 2048; ",
                        "openssl req -new -key {} -subj {} -config {} -extensions v3_req -out {}; ",
                        "openssl x509 -req -in {} -CA {} -CAkey {} -CAcreateserial -days 365 -out {} -extfile {}"
                    ),
                    shell_quote_path(&key_path),
                    shell_quote_path(&key_path),
                    shell_quote(&format!("/CN={}", host.member_name)),
                    shell_quote_path(&csr_config_path),
                    shell_quote_path(&csr_path),
                    shell_quote_path(&csr_path),
                    shell_quote_path(&ca_cert_path),
                    shell_quote_path(&ca_key_path),
                    shell_quote_path(&crt_path),
                    shell_quote_path(&san_ext_path),
                ),
                chdir: None,
                creates: Some(crt_path),
                removes: None,
                stdin: None,
            })
            .await?;
    }

    Ok(())
}

async fn install_etcd_runtime(
    local: &mut Local,
    inventory: &mut Inventory,
    local_download_dir: &Path,
    version: &str,
    remote_ssl_dir: &str,
    remote_data_dir: &str,
    remote_service_path: &str,
) -> anyhow::Result<()> {
    let local_archive_path = local_download_dir.join(format!("etcd-{version}-linux-amd64.tar.gz"));
    let archive_path = PathBuf::from(format!("/tmp/etcd-{version}-linux-amd64.tar.gz"));
    let extract_dir = PathBuf::from(format!("/tmp/etcd-{version}-linux-amd64"));

    local
        .run(FileTask {
            path: local_download_dir.to_path_buf(),
            state: FileState::Directory,
            owner: None,
            group: None,
            mode: Some("0755".to_string()),
            content: None,
        })
        .await?;

    inventory
        .run(UserTask {
            name: "etcd".to_string(),
            system: true,
            create_home: false,
            shell: Some(PathBuf::from("/usr/sbin/nologin")),
            home: None,
        })
        .await?;

    for path in [remote_ssl_dir, remote_data_dir] {
        inventory
            .run(FileTask {
                path: PathBuf::from(path),
                state: FileState::Directory,
                owner: Some("etcd".to_string()),
                group: Some("etcd".to_string()),
                mode: Some("0755".to_string()),
                content: None,
            })
            .await?;
    }

    local
        .run(DownloadTask {
            url: format!(
                "https://github.com/etcd-io/etcd/releases/download/{version}/etcd-{version}-linux-amd64.tar.gz"
            ),
            dest: local_archive_path.clone(),
            force: false,
            owner: None,
            group: None,
            mode: Some("0644".to_string()),
        })
        .await?;

    for upload in inventory
        .upload_file(&local_archive_path, &archive_path, UploadOptions::default())
        .await?
    {
        info!(
            host = %upload.host,
            local_path = %upload.local_path.display(),
            remote_path = %upload.remote_path,
            bytes = upload.bytes_written,
            "uploaded etcd archive to remote host"
        );
    }

    inventory
        .run(UnarchiveTask {
            src: archive_path,
            dest: PathBuf::from("/tmp"),
            creates: Some(extract_dir.join("etcd")),
        })
        .await?;

    for binary in ["etcd", "etcdctl"] {
        inventory
            .run(CopyTask {
                src: extract_dir.join(binary),
                dest: PathBuf::from(format!("/usr/local/bin/{binary}")),
                owner: None,
                group: None,
                mode: Some("0755".to_string()),
            })
            .await?;
    }

    inventory
        .run(FileTask {
            path: PathBuf::from(remote_service_path),
            state: FileState::Touch,
            owner: None,
            group: None,
            mode: Some("0644".to_string()),
            content: None,
        })
        .await?;

    Ok(())
}

async fn distribute_certificates(inventory: &Inventory) -> anyhow::Result<()> {
    for upload in inventory
        .upload_file(
            TemplatePath::new("{{ etcd.local_cert_dir }}/ca.crt"),
            TemplatePath::new("{{ etcd.remote_ssl_dir }}/ca.crt"),
            UploadOptions {
                owner: Some("etcd".to_string()),
                group: Some("etcd".to_string()),
                mode: Some("0600".to_string()),
            },
        )
        .await?
    {
        info!(
            host = %upload.host,
            local_path = %upload.local_path.display(),
            remote_path = %upload.remote_path,
            bytes = upload.bytes_written,
            "uploaded file to remote host"
        );
    }

    for upload in inventory
        .upload_file(
            TemplatePath::new("{{ etcd.local_cert_dir }}/{{ rusible.host.name }}.crt"),
            TemplatePath::new("{{ etcd.remote_ssl_dir }}/server.crt"),
            UploadOptions {
                owner: Some("etcd".to_string()),
                group: Some("etcd".to_string()),
                mode: Some("0600".to_string()),
            },
        )
        .await?
    {
        info!(
            host = %upload.host,
            local_path = %upload.local_path.display(),
            remote_path = %upload.remote_path,
            bytes = upload.bytes_written,
            "uploaded file to remote host"
        );
    }

    for upload in inventory
        .upload_file(
            TemplatePath::new("{{ etcd.local_cert_dir }}/{{ rusible.host.name }}.key"),
            TemplatePath::new("{{ etcd.remote_ssl_dir }}/server.key"),
            UploadOptions {
                owner: Some("etcd".to_string()),
                group: Some("etcd".to_string()),
                mode: Some("0600".to_string()),
            },
        )
        .await?
    {
        info!(
            host = %upload.host,
            local_path = %upload.local_path.display(),
            remote_path = %upload.remote_path,
            bytes = upload.bytes_written,
            "uploaded file to remote host"
        );
    }

    Ok(())
}

fn host_spec_from_inventory_host(host: &Host) -> anyhow::Result<EtcdHostSpec> {
    let member_name = host.remote().get_var("etcd.name")?;
    let peer_host = optional_string(host.remote().vars(), "etcd.peer_host")
        .unwrap_or_else(|| host.remote().host.clone());
    let client_host = optional_string(host.remote().vars(), "etcd.client_host")
        .unwrap_or_else(|| peer_host.clone());

    Ok(EtcdHostSpec {
        inventory_name: host.name().to_string(),
        member_name,
        peer_host,
        client_host,
    })
}

fn build_cert_sans(host: &EtcdHostSpec) -> String {
    let mut entries = vec![format!("DNS:{}", host.member_name), "IP:127.0.0.1".to_string()];

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

fn optional_string(vars: &rusible::Table, path: &str) -> Option<String> {
    lookup_value(vars, path).and_then(|value| value.as_str().map(ToOwned::to_owned))
}

fn lookup_value<'a>(vars: &'a rusible::Table, path: &str) -> Option<&'a rusible::Value> {
    let mut parts = path.split('.');
    let first = parts.next()?;
    let mut value = vars.get(first)?;

    for part in parts {
        value = value.as_table()?.get(part)?;
    }

    Some(value)
}

fn shell_quote_path(path: &Path) -> String {
    shell_quote(&path.display().to_string())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
