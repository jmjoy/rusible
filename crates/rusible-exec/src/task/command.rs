use crate::Error;
use rusible_meta::{CommandDetails, CommandTaskData, TaskDetails, TaskResult, TaskStatus};
use std::process::Stdio;
use tokio::{fs, io::AsyncWriteExt, process::Command};

pub(crate) async fn execute(task: &CommandTaskData) -> Result<TaskResult, Error> {
    let argv = task.argv.clone();

    if let Some(path) = task.creates.as_deref()
        && fs::try_exists(path).await?
    {
        return Ok(task_result(
            TaskStatus::Skipped,
            format!(
                "command not run because creates path {} already exists",
                path.display()
            ),
            CommandDetails {
                cmd: argv.clone(),
                chdir: task.chdir.clone(),
                rc: None,
                stdout: String::new(),
                stderr: String::new(),
            },
        ));
    }

    if let Some(path) = task.removes.as_deref()
        && !fs::try_exists(path).await?
    {
        return Ok(task_result(
            TaskStatus::Skipped,
            format!(
                "command not run because removes path {} is already absent",
                path.display()
            ),
            CommandDetails {
                cmd: argv.clone(),
                chdir: task.chdir.clone(),
                rc: None,
                stdout: String::new(),
                stderr: String::new(),
            },
        ));
    }

    let mut command = Command::new(&argv[0]);
    command
        .args(&argv[1..])
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
                format!("failed to spawn command: {error}"),
                CommandDetails {
                    cmd: argv.clone(),
                    chdir: task.chdir.clone(),
                    rc: None,
                    stdout: String::new(),
                    stderr: error.to_string(),
                },
            ));
        }
    };

    if let Some(stdin) = &task.stdin
        && let Some(mut child_stdin) = child.stdin.take()
    {
        child_stdin.write_all(stdin.as_bytes()).await?;
    }

    let output = child.wait_with_output().await?;
    let rc = output.status.code();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let details = CommandDetails {
        cmd: argv,
        chdir: task.chdir.clone(),
        rc,
        stdout,
        stderr,
    };

    if output.status.success() {
        Ok(task_result(
            TaskStatus::Changed,
            "command executed",
            details,
        ))
    } else {
        Ok(task_result(
            TaskStatus::Failed,
            format!("command failed with status {}", rc.unwrap_or(-1)),
            details,
        ))
    }
}

fn task_result(
    status: TaskStatus, message: impl Into<String>, details: CommandDetails,
) -> TaskResult {
    TaskResult {
        status,
        message: Some(message.into()),
        details: Some(TaskDetails::Command(details)),
    }
}

#[cfg(test)]
mod tests {
    use super::execute;
    use rusible_meta::{CommandDetails, CommandTaskData, TaskDetails, TaskStatus};
    use std::{
        env, fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[tokio::test(flavor = "current_thread")]
    async fn execute_command_task_skips_when_creates_exists() {
        let path = unique_temp_path("creates");
        fs::write(&path, "exists").unwrap();

        let result = execute(&CommandTaskData {
            name: None,
            argv: vec!["false".to_string()],
            chdir: None,
            creates: Some(path.clone()),
            removes: None,
            stdin: None,
        })
        .await
        .unwrap();

        assert_eq!(result.status, TaskStatus::Skipped);

        fs::remove_file(path).unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn execute_command_task_marks_success_as_changed() {
        let result = execute(&CommandTaskData {
            name: None,
            argv: vec!["true".to_string()],
            chdir: None,
            creates: None,
            removes: None,
            stdin: None,
        })
        .await
        .unwrap();

        assert_eq!(result.status, TaskStatus::Changed);
        assert!(matches!(
            result.details,
            Some(TaskDetails::Command(CommandDetails { rc: Some(0), .. }))
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn execute_command_task_marks_non_zero_exit_as_failed() {
        let result = execute(&CommandTaskData {
            name: None,
            argv: vec!["false".to_string()],
            chdir: None,
            creates: None,
            removes: None,
            stdin: None,
        })
        .await
        .unwrap();

        assert_eq!(result.status, TaskStatus::Failed);
        assert!(matches!(
            result.details,
            Some(TaskDetails::Command(CommandDetails { rc: Some(1), .. }))
        ));
    }

    fn unique_temp_path(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("rusible-exec-{prefix}-{stamp}"))
    }
}
