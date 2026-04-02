mod task;

use clap::Parser;
use rusible_meta::{TaskRequest, TaskResult, TaskValidationError};
use std::{io, process::ExitCode};
use tokio::io::AsyncReadExt;

#[derive(Debug, Parser)]
#[command(name = "rusible-exec", version, about, long_about = None)]
struct Cli {}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Http(#[from] reqwest::Error),

    #[error(transparent)]
    Validation(#[from] TaskValidationError),

    #[error("command `{program}` failed with status {status}: {stderr}")]
    CommandFailed {
        program: String,
        status: i32,
        stderr: String,
    },

    #[error("timed out waiting for {host}:{port} after {timeout_secs}s")]
    WaitForTimeout {
        host: String,
        port: u16,
        timeout_secs: u64,
    },
}

fn main() -> ExitCode {
    let _ = Cli::parse();

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let result = TaskResult::failed(error.to_string());
            print_result(&result);
            return ExitCode::from(2);
        }
    };

    match runtime.block_on(run()) {
        Ok(result) => {
            print_result(&result);
            match result.status {
                rusible_meta::TaskStatus::Ok
                | rusible_meta::TaskStatus::Changed
                | rusible_meta::TaskStatus::Skipped => ExitCode::SUCCESS,
                rusible_meta::TaskStatus::Failed | rusible_meta::TaskStatus::Unreachable => {
                    ExitCode::from(2)
                }
            }
        }
        Err(error) => {
            let result = TaskResult::failed(error.to_string());
            print_result(&result);
            ExitCode::from(2)
        }
    }
}

async fn run() -> Result<TaskResult, Error> {
    let mut stdin = String::new();
    let mut input = tokio::io::stdin();
    input.read_to_string(&mut stdin).await?;

    let request: TaskRequest = serde_json::from_str(&stdin)?;
    task::execute(request).await
}

fn print_result(result: &TaskResult) {
    println!(
        "{}",
        serde_json::to_string(result).expect("serializing task result")
    );
}
