use rusible::{
    Field, init_forest_logging,
    meta::{FileState, FileTask},
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
        .run(FileTask {
            name: "Render hello world template".into(),
            path: PathBuf::from("/tmp/hello-world.txt").into(),
            state: FileState::File.into(),
            content: Field::tpl(include_str!("hello-world.j2")),
            mode: "0644".into(),
            ..Default::default()
        })
        .await?;

    Ok(())
}
