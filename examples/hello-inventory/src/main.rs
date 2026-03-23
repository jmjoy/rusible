use rusible::{inventory::Inventory, meta::TemplateTask, runtime::Runnable as _};
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::EnvFilter;

const RUSIBLE_EXEC_BYTES: &[u8] = include_bytes!(env!("CARGO_BIN_FILE_RUSIBLE_EXEC"));

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,rusible=debug")),
        )
        .init();

    let inventory_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("inventory.toml");
    let mut inventory = Inventory::from_toml_path(&inventory_path)
        .await?
        .filter_by_group("web");

    inventory.set_var("app.release", "2026.03")?;
    inventory
        .host_mut("ops-1")
        .expect("ops-1 should exist in the example inventory")
        .remote_mut()
        .set_var("app.role", "ops-web")?;

    info!(
        inventory = %inventory_path.display(),
        selected_hosts = inventory.len(),
        "loaded inventory and selected hosts"
    );

    inventory.init(RUSIBLE_EXEC_BYTES).await?;

    let report = inventory
        .run(TemplateTask {
            dest: PathBuf::from("/tmp/hello-inventory.txt"),
            content: concat!(
                "hello from {{ app.name }}\n",
                "env={{ app.env }}\n",
                "role={{ app.role }}\n",
                "release={{ app.release }}\n",
                "group={{ group.primary }}\n",
                "inventory_host={{ rusible.host.name }}\n",
                "ssh={{ rusible.host.user }}@{{ rusible.host.host }}:{{ rusible.host.port }}\n"
            )
            .to_string(),
            owner: None,
            group: None,
            mode: Some("0644".to_string()),
        })
        .await?;

    for result in report.0 {
        info!(
            host = %result.host,
            exec_path = %result.exec_path,
            status = ?result.result.status,
            message = result.result.message.as_deref().unwrap_or(""),
            "template task finished"
        );
    }

    Ok(())
}
