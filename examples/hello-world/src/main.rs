use rusible::{meta::TemplateTask, runtime::Runnable as _, target::Remote};
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

    let mut remotes = vec![
        Remote::new("127.0.0.1", 2222, "root", Some("123456".into()), None),
        Remote::new("127.0.0.1", 2223, "root", Some("123456".into()), None),
        Remote::new("127.0.0.1", 2224, "root", Some("123456".into()), None),
    ];
    remotes.init(RUSIBLE_EXEC_BYTES).await?;

    let report = remotes
        .run(TemplateTask {
            dest: PathBuf::from("/tmp/hello-world.txt"),
            content: "hello world from rusible\n".to_string(),
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
