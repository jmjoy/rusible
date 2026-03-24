use rusible::{
    init_forest_logging,
    inventory::Inventory,
    meta::TemplateTask,
    runtime::Runnable as _,
};
use std::path::PathBuf;

const RUSIBLE_EXEC_BYTES: &[u8] = include_bytes!(env!("CARGO_BIN_FILE_RUSIBLE_EXEC"));

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_forest_logging("info,rusible=debug");

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

    eprintln!(
        "loaded inventory {} with {} selected hosts",
        inventory_path.display(),
        inventory.len(),
    );

    inventory.init(RUSIBLE_EXEC_BYTES).await?;

    inventory
        .run(TemplateTask {
            name: Some("Render inventory hello template".to_string()),
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

    Ok(())
}
