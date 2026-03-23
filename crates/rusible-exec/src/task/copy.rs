use super::file;
use crate::Error;
use rusible_meta::{CopyDetails, CopyTask, TaskDetails, TaskResult, TaskStatus};
use tokio::fs;

pub(crate) async fn execute(task: &CopyTask) -> Result<TaskResult, Error> {
    let mut changes = file::FileChangeSummary::default();

    let source = fs::read(&task.src).await?;

    if let Some(parent) = task.dest.parent() {
        if !parent.as_os_str().is_empty() && !fs::try_exists(parent).await? {
            fs::create_dir_all(parent).await?;
        }
    }

    match fs::read(&task.dest).await {
        Ok(current) => {
            if current != source {
                fs::write(&task.dest, &source).await?;
                changes.content_changed = true;
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::write(&task.dest, &source).await?;
            changes.created = true;
            changes.content_changed = true;
        }
        Err(error) => return Err(error.into()),
    }

    changes.mode_changed = file::apply_mode(&task.dest, task.mode.as_deref()).await?;
    changes.ownership_changed =
        file::apply_owner_group(&task.dest, task.owner.as_deref(), task.group.as_deref()).await?;

    let details = CopyDetails {
        src: task.src.clone(),
        dest: task.dest.clone(),
        created: changes.created,
        content_changed: changes.content_changed,
        mode_changed: changes.mode_changed,
        ownership_changed: changes.ownership_changed,
    };

    Ok(TaskResult {
        status: if changes.any() { TaskStatus::Changed } else { TaskStatus::Ok },
        message: Some(if changes.any() {
            format!("copied {} to {}", task.src.display(), task.dest.display())
        } else {
            format!("{} already matches {}", task.dest.display(), task.src.display())
        }),
        details: Some(TaskDetails::Copy(details)),
    })
}
