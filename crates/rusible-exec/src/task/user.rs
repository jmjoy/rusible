use crate::Error;
use rusible_meta::{TaskDetails, TaskResult, TaskStatus, UserDetails, UserTask};
use tokio::process::Command;

pub(crate) async fn execute(task: &UserTask) -> Result<TaskResult, Error> {
    if user_exists(&task.username).await? {
        return Ok(TaskResult {
            status: TaskStatus::Ok,
            message: Some(format!("user {} already exists", task.username)),
            details: Some(TaskDetails::User(UserDetails {
                name: task.username.clone(),
                exists: true,
                created: false,
            })),
        });
    }

    let mut command = Command::new("useradd");
    if task.system {
        command.arg("--system");
    }
    if !task.create_home {
        command.arg("--no-create-home");
    }
    if let Some(shell) = &task.shell {
        command.arg("--shell").arg(shell);
    }
    if let Some(home) = &task.home {
        command.arg("--home-dir").arg(home);
    }
    command.arg(&task.username);

    let output = command.output().await?;
    if !output.status.success() {
        return Err(Error::CommandFailed {
            program: "useradd".to_string(),
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    Ok(TaskResult {
        status: TaskStatus::Changed,
        message: Some(format!("created user {}", task.username)),
        details: Some(TaskDetails::User(UserDetails {
            name: task.username.clone(),
            exists: true,
            created: true,
        })),
    })
}

async fn user_exists(name: &str) -> Result<bool, Error> {
    let output = Command::new("getent")
        .arg("passwd")
        .arg(name)
        .output()
        .await?;
    Ok(output.status.success())
}
