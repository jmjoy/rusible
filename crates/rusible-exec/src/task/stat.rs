use crate::Error;
use rusible_meta::{StatDetails, StatTaskData, TaskDetails, TaskResult, TaskStatus};
use std::os::unix::fs::PermissionsExt;
use tokio::fs;

pub(crate) async fn execute(task: &StatTaskData) -> Result<TaskResult, Error> {
    let details = match fs::symlink_metadata(&task.path).await {
        Ok(metadata) => StatDetails {
            path: task.path.clone(),
            exists: true,
            is_file: metadata.is_file(),
            is_dir: metadata.is_dir(),
            is_symlink: metadata.file_type().is_symlink(),
            mode: Some(format!("{:04o}", metadata.permissions().mode() & 0o7777)),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => StatDetails {
            path: task.path.clone(),
            exists: false,
            is_file: false,
            is_dir: false,
            is_symlink: false,
            mode: None,
        },
        Err(error) => return Err(error.into()),
    };

    let message = if details.exists {
        format!("{} exists", task.path.display())
    } else {
        format!("{} is absent", task.path.display())
    };

    Ok(TaskResult {
        status: TaskStatus::Ok,
        message: Some(message),
        details: Some(TaskDetails::Stat(details)),
    })
}
