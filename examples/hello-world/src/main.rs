use rusible::{
    init_forest_logging,
    meta::TemplateTask,
    runtime::Runnable as _,
    target::Remote,
    toml,
};
use std::path::PathBuf;

const RUSIBLE_EXEC_BYTES: &[u8] = include_bytes!(env!("CARGO_BIN_FILE_RUSIBLE_EXEC"));

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_forest_logging("info,rusible=debug");

    let mut remotes = vec![
        Remote::new_with_vars(
            "127.0.0.1",
            2222,
            "root",
            Some("123456".into()),
            None,
            toml! {
                [app]
                name = "rusible"
                instance = "node-1"
            },
        ),
        Remote::new_with_vars(
            "127.0.0.1",
            2223,
            "root",
            Some("123456".into()),
            None,
            toml! {
                [app]
                name = "rusible"
                instance = "node-2"
            },
        ),
        Remote::new_with_vars(
            "127.0.0.1",
            2224,
            "root",
            Some("123456".into()),
            None,
            toml! {
                [app]
                name = "rusible"
                instance = "node-3"
            },
        ),
    ];

    remotes.init(RUSIBLE_EXEC_BYTES).await?;

    remotes
        .run(TemplateTask {
            name: Some("Render hello world template".to_string()),
            dest: PathBuf::from("/tmp/hello-world.txt"),
            content: include_str!("hello-world.j2").to_string(),
            owner: None,
            group: None,
            mode: Some("0644".to_string()),
        })
        .await?;

    Ok(())
}
