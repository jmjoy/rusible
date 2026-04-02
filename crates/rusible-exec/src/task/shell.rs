use crate::Error;
use rusible_meta::{ShellDetails, ShellTaskData, TaskDetails, TaskResult, TaskStatus};
use std::process::Stdio;
use tokio::{fs, io::AsyncWriteExt, process::Command};

pub(crate) async fn execute(task: &ShellTaskData) -> Result<TaskResult, Error> {
    if let Some(path) = task.creates.as_deref()
        && fs::try_exists(path).await? {
            return Ok(task_result(
                TaskStatus::Skipped,
                format!(
                    "shell command not run because creates path {} already exists",
                    path.display()
                ),
                ShellDetails {
                    cmd: task.cmd.clone(),
                    chdir: task.chdir.clone(),
                    rc: None,
                    stdout: String::new(),
                    stderr: String::new(),
                },
            ));
        }

    if let Some(path) = task.removes.as_deref()
        && !fs::try_exists(path).await? {
            return Ok(task_result(
                TaskStatus::Skipped,
                format!(
                    "shell command not run because removes path {} is already absent",
                    path.display()
                ),
                ShellDetails {
                    cmd: task.cmd.clone(),
                    chdir: task.chdir.clone(),
                    rc: None,
                    stdout: String::new(),
                    stderr: String::new(),
                },
            ));
        }

    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(&task.cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if task.stdin.is_some() {
        command.stdin(Stdio::piped());
    }

    if let Some(chdir) = &task.chdir {
        command.current_dir(chdir);
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return Ok(task_result(
                TaskStatus::Failed,
                format!("failed to spawn shell command: {error}"),
                ShellDetails {
                    cmd: task.cmd.clone(),
                    chdir: task.chdir.clone(),
                    rc: None,
                    stdout: String::new(),
                    stderr: error.to_string(),
                },
            ));
        }
    };

    if let Some(stdin) = &task.stdin
        && let Some(mut child_stdin) = child.stdin.take() {
            child_stdin.write_all(stdin.as_bytes()).await?;
        }

    let output = child.wait_with_output().await?;
    let rc = output.status.code();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let details = ShellDetails {
        cmd: task.cmd.clone(),
        chdir: task.chdir.clone(),
        rc,
        stdout,
        stderr,
    };

    if output.status.success() {
        Ok(task_result(
            TaskStatus::Changed,
            "shell command executed",
            details,
        ))
    } else {
        Ok(task_result(
            TaskStatus::Failed,
            format!("shell command failed with status {}", rc.unwrap_or(-1)),
            details,
        ))
    }
}

fn task_result(
    status: TaskStatus, message: impl Into<String>, details: ShellDetails,
) -> TaskResult {
    TaskResult {
        status,
        message: Some(message.into()),
        details: Some(TaskDetails::Shell(details)),
    }
}
