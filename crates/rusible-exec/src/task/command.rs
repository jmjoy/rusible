use crate::Error;
use rusible_meta::{CommandDetails, CommandTask, TaskDetails, TaskResult, TaskStatus};
use std::{io, process::Stdio};
use tokio::{
    fs,
    io::AsyncWriteExt,
    process::Command,
};

pub(crate) async fn execute(task: &CommandTask) -> Result<TaskResult, Error> {
    let argv = resolve_argv(task)?;

    if let Some(path) = task.creates.as_deref() {
        if fs::try_exists(path).await? {
            return Ok(task_result(
                TaskStatus::Skipped,
                format!(
                    "command not run because creates path {} already exists",
                    path.display()
                ),
                CommandDetails {
                    cmd: argv,
                    chdir: task.chdir.clone(),
                    rc: None,
                    stdout: String::new(),
                    stderr: String::new(),
                },
            ));
        }
    }

    if let Some(path) = task.removes.as_deref() {
        if !fs::try_exists(path).await? {
            return Ok(task_result(
                TaskStatus::Skipped,
                format!(
                    "command not run because removes path {} is already absent",
                    path.display()
                ),
                CommandDetails {
                    cmd: argv,
                    chdir: task.chdir.clone(),
                    rc: None,
                    stdout: String::new(),
                    stderr: String::new(),
                },
            ));
        }
    }

    let mut command = Command::new(&argv[0]);
    command.args(&argv[1..]).stdout(Stdio::piped()).stderr(Stdio::piped());

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
                    cmd: argv,
                    chdir: task.chdir.clone(),
                    rc: None,
                    stdout: String::new(),
                    stderr: error.to_string(),
                },
            ));
        }
    };

    if let Some(stdin) = &task.stdin {
        if let Some(mut child_stdin) = child.stdin.take() {
            child_stdin.write_all(stdin.as_bytes()).await?;
        }
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

fn resolve_argv(task: &CommandTask) -> Result<Vec<String>, Error> {
    match (task.cmd.as_deref(), task.argv.as_ref()) {
        (Some(cmd), None) => {
            let argv = shlex::split(cmd).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("invalid command string: {cmd}"),
                )
            })?;

            if argv.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "command string must not be empty",
                )
                .into());
            }

            Ok(argv)
        }
        (None, Some(argv)) if !argv.is_empty() => Ok(argv.clone()),
        (Some(_), Some(_)) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "command task accepts either cmd or argv, not both",
        )
        .into()),
        (None, Some(_)) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "command argv must not be empty",
        )
        .into()),
        (None, None) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "command task requires either cmd or argv",
        )
        .into()),
    }
}

fn task_result(status: TaskStatus, message: impl Into<String>, details: CommandDetails) -> TaskResult {
    TaskResult {
        status,
        message: Some(message.into()),
        details: Some(TaskDetails::Command(details)),
    }
}

#[cfg(test)]
mod tests {
    use super::{execute, resolve_argv};
    use rusible_meta::{CommandDetails, CommandTask, TaskDetails, TaskStatus};
    use std::{env, fs, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};

    #[test]
    fn resolve_command_argv_from_cmd_string() {
        let argv = resolve_argv(&CommandTask {
            cmd: Some("echo 'hello world'".to_string()),
            argv: None,
            chdir: None,
            creates: None,
            removes: None,
            stdin: None,
        })
        .unwrap();

        assert_eq!(argv, vec!["echo".to_string(), "hello world".to_string()]);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn execute_command_task_skips_when_creates_exists() {
        let path = unique_temp_path("creates");
        fs::write(&path, "exists").unwrap();

        let result = execute(&CommandTask {
            cmd: Some("false".to_string()),
            argv: None,
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
        let result = execute(&CommandTask {
            cmd: None,
            argv: Some(vec!["true".to_string()]),
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
        let result = execute(&CommandTask {
            cmd: None,
            argv: Some(vec!["false".to_string()]),
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
