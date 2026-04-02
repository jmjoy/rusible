use crate::Error;
use rusible_meta::task::{
    TaskDetails, TaskResult, TaskStatus,
    systemd::{SystemdDetails, SystemdState, SystemdTaskData},
};
use tokio::process::Command;

pub(crate) async fn execute(task: &SystemdTaskData) -> Result<TaskResult, Error> {
    let mut changed = false;

    if task.daemon_reload {
        run_systemctl(["daemon-reload"]).await?;
        changed = true;
    }

    if let Some(enabled) = task.enabled {
        let current = is_enabled(&task.unit).await?;
        if current != Some(enabled) {
            run_systemctl([if enabled { "enable" } else { "disable" }, &task.unit]).await?;
            changed = true;
        }
    }

    if let Some(state) = task.state {
        match state {
            SystemdState::Started => {
                if is_active(&task.unit).await? != Some(true) {
                    run_systemctl(["start", &task.unit]).await?;
                    changed = true;
                }
            }
            SystemdState::Stopped => {
                if is_active(&task.unit).await? != Some(false) {
                    run_systemctl(["stop", &task.unit]).await?;
                    changed = true;
                }
            }
            SystemdState::Restarted => {
                run_systemctl(["restart", &task.unit]).await?;
                changed = true;
            }
            SystemdState::Reloaded => {
                run_systemctl(["reload", &task.unit]).await?;
                changed = true;
            }
        }
    }

    let details = SystemdDetails {
        unit: task.unit.clone(),
        active: is_active(&task.unit).await?,
        enabled: is_enabled(&task.unit).await?,
        changed,
    };

    Ok(TaskResult {
        status: if changed {
            TaskStatus::Changed
        } else {
            TaskStatus::Ok
        },
        message: Some(if changed {
            format!("systemd unit {} updated", task.unit)
        } else {
            format!("systemd unit {} already in desired state", task.unit)
        }),
        details: Some(TaskDetails::Systemd(details)),
    })
}

async fn run_systemctl<const N: usize>(args: [&str; N]) -> Result<(), Error> {
    let output = Command::new("systemctl").args(args).output().await?;
    if output.status.success() {
        return Ok(());
    }

    Err(Error::CommandFailed {
        program: format!("systemctl {}", args.join(" ")),
        status: output.status.code().unwrap_or(-1),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })
}

async fn is_active(unit: &str) -> Result<Option<bool>, Error> {
    let output = Command::new("systemctl")
        .args(["is-active", unit])
        .output()
        .await?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    if output.status.success() {
        Ok(Some(true))
    } else if stdout.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(false))
    }
}

async fn is_enabled(unit: &str) -> Result<Option<bool>, Error> {
    let output = Command::new("systemctl")
        .args(["is-enabled", unit])
        .output()
        .await?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let state = stdout.trim();

    if output.status.success() {
        Ok(Some(true))
    } else if matches!(state, "disabled" | "static" | "indirect" | "masked") {
        Ok(Some(false))
    } else {
        Ok(None)
    }
}
