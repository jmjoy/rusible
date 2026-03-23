mod task;

use clap::Parser;
use rusible_meta::{TaskRequest, TaskResult};
use std::{
    io::{self, Read},
    process::ExitCode,
};

#[derive(Debug, Parser)]
#[command(name = "rusible-exec", version, about, long_about = None)]
struct Cli {}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error("template rendering failed: {0}")]
    Template(#[from] minijinja::Error),

    #[error("command `{program}` failed with status {status}: {stderr}")]
    CommandFailed {
        program: String,
        status: i32,
        stderr: String,
    },
}

fn main() -> ExitCode {
    let _ = Cli::parse();

    match run() {
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

fn run() -> Result<TaskResult, Error> {
    let mut stdin = String::new();
    io::stdin().read_to_string(&mut stdin)?;

    let request: TaskRequest = serde_json::from_str(&stdin)?;
    task::execute(request)
}

fn print_result(result: &TaskResult) {
    println!(
        "{}",
        serde_json::to_string(result).expect("serializing task result")
    );
}
