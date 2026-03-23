use crate::Error;
use rusible_meta::{FileDetails, FileState, FileTask, TaskDetails, TaskResult, TaskStatus};
use std::{
    os::unix::fs::PermissionsExt,
    path::Path,
};
use tokio::{
    fs::{self, OpenOptions},
    process::Command,
};

#[derive(Debug, Default)]
pub(crate) struct FileChangeSummary {
    pub(crate) created: bool,
    pub(crate) removed: bool,
    pub(crate) content_changed: bool,
    pub(crate) mode_changed: bool,
    pub(crate) ownership_changed: bool,
}

impl FileChangeSummary {
    pub(crate) fn any(&self) -> bool {
        self.created
            || self.removed
            || self.content_changed
            || self.mode_changed
            || self.ownership_changed
    }

    fn into_file_details(self, path: &Path, state: FileState) -> FileDetails {
        FileDetails {
            path: path.to_path_buf(),
            state,
            created: self.created,
            removed: self.removed,
            content_changed: self.content_changed,
            mode_changed: self.mode_changed,
            ownership_changed: self.ownership_changed,
        }
    }

    pub(crate) fn into_template_details(self, dest: &Path) -> rusible_meta::TemplateDetails {
        rusible_meta::TemplateDetails {
            dest: dest.to_path_buf(),
            created: self.created,
            content_changed: self.content_changed,
            mode_changed: self.mode_changed,
            ownership_changed: self.ownership_changed,
        }
    }
}

pub(crate) async fn execute(task: &FileTask) -> Result<TaskResult, Error> {
    match task.state {
        FileState::Absent => ensure_absent(&task.path).await,
        FileState::Directory => ensure_directory(task).await,
        FileState::File => ensure_file(task).await,
        FileState::Touch => ensure_touch(task).await,
    }
}

async fn ensure_absent(path: &Path) -> Result<TaskResult, Error> {
    let metadata = match fs::symlink_metadata(path).await {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };

    let Some(metadata) = metadata else {
        return Ok(task_result(
            TaskStatus::Ok,
            format!("{} is already absent", path.display()),
            FileChangeSummary::default().into_file_details(path, FileState::Absent),
        ));
    };

    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path).await?;
    } else {
        fs::remove_file(path).await?;
    }

    Ok(task_result(
        TaskStatus::Changed,
        format!("removed {}", path.display()),
        FileChangeSummary {
            removed: true,
            ..FileChangeSummary::default()
        }
        .into_file_details(path, FileState::Absent),
    ))
}

async fn ensure_directory(task: &FileTask) -> Result<TaskResult, Error> {
    let mut changes = FileChangeSummary::default();

    if !matches!(fs::symlink_metadata(&task.path).await, Ok(metadata) if metadata.is_dir()) {
        fs::create_dir_all(&task.path).await?;
        changes.created = true;
    }

    changes.mode_changed = apply_mode(&task.path, task.mode.as_deref()).await?;
    changes.ownership_changed =
        apply_owner_group(&task.path, task.owner.as_deref(), task.group.as_deref()).await?;

    let status = if changes.any() {
        TaskStatus::Changed
    } else {
        TaskStatus::Ok
    };
    let message = if changes.any() {
        format!("directory {} updated", task.path.display())
    } else {
        format!("directory {} already in desired state", task.path.display())
    };

    Ok(task_result(
        status,
        message,
        changes.into_file_details(&task.path, FileState::Directory),
    ))
}

async fn ensure_file(task: &FileTask) -> Result<TaskResult, Error> {
    let mut changes = FileChangeSummary::default();

    if let Some(parent) = task.path.parent() {
        if !parent.as_os_str().is_empty() && !fs::try_exists(parent).await? {
            fs::create_dir_all(parent).await?;
        }
    }

    if !fs::try_exists(&task.path).await? {
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&task.path)
            .await?;
        changes.created = true;
    }

    if let Some(content) = &task.content {
        let current = fs::read_to_string(&task.path).await.ok();
        if current.as_deref() != Some(content.as_str()) {
            fs::write(&task.path, content).await?;
            changes.content_changed = true;
        }
    }

    changes.mode_changed = apply_mode(&task.path, task.mode.as_deref()).await?;
    changes.ownership_changed =
        apply_owner_group(&task.path, task.owner.as_deref(), task.group.as_deref()).await?;

    let status = if changes.any() {
        TaskStatus::Changed
    } else {
        TaskStatus::Ok
    };
    let message = if changes.any() {
        format!("file {} updated", task.path.display())
    } else {
        format!("file {} already in desired state", task.path.display())
    };

    Ok(task_result(
        status,
        message,
        changes.into_file_details(&task.path, FileState::File),
    ))
}

async fn ensure_touch(task: &FileTask) -> Result<TaskResult, Error> {
    let existed = fs::try_exists(&task.path).await?;

    if let Some(parent) = task.path.parent() {
        if !parent.as_os_str().is_empty() && !fs::try_exists(parent).await? {
            fs::create_dir_all(parent).await?;
        }
    }

    let status = Command::new("touch").arg(&task.path).status().await?;
    if !status.success() {
        return Err(Error::CommandFailed {
            program: "touch".to_string(),
            status: status.code().unwrap_or(-1),
            stderr: String::new(),
        });
    }

    let mut changes = FileChangeSummary {
        created: !existed,
        ..FileChangeSummary::default()
    };
    changes.mode_changed = apply_mode(&task.path, task.mode.as_deref()).await?;
    changes.ownership_changed =
        apply_owner_group(&task.path, task.owner.as_deref(), task.group.as_deref()).await?;

    let status = if changes.any() {
        TaskStatus::Changed
    } else {
        TaskStatus::Ok
    };
    let message = if changes.any() {
        format!("touched {}", task.path.display())
    } else {
        format!("file {} already in desired state", task.path.display())
    };

    Ok(task_result(
        status,
        message,
        changes.into_file_details(&task.path, FileState::Touch),
    ))
}

pub(crate) async fn apply_mode(path: &Path, mode: Option<&str>) -> Result<bool, Error> {
    let Some(mode) = mode else {
        return Ok(false);
    };

    let desired = u32::from_str_radix(mode, 8).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("invalid octal mode `{mode}`: {error}"),
        )
    })?;

    let metadata = fs::metadata(path).await?;
    let current = metadata.permissions().mode() & 0o7777;
    if current == desired {
        return Ok(false);
    }

    let mut permissions = metadata.permissions();
    permissions.set_mode(desired);
    fs::set_permissions(path, permissions).await?;
    Ok(true)
}

pub(crate) async fn apply_owner_group(
    path: &Path,
    owner: Option<&str>,
    group: Option<&str>,
) -> Result<bool, Error> {
    let Some(spec) = build_owner_group_spec(owner, group) else {
        return Ok(false);
    };

    let output = Command::new("chown").arg(&spec).arg(path).output().await?;
    if !output.status.success() {
        return Err(Error::CommandFailed {
            program: "chown".to_string(),
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    Ok(true)
}

fn task_result(status: TaskStatus, message: impl Into<String>, details: FileDetails) -> TaskResult {
    TaskResult {
        status,
        message: Some(message.into()),
        details: Some(TaskDetails::File(details)),
    }
}

pub(crate) fn build_owner_group_spec(owner: Option<&str>, group: Option<&str>) -> Option<String> {
    match (owner, group) {
        (Some(owner), Some(group)) => Some(format!("{owner}:{group}")),
        (Some(owner), None) => Some(owner.to_string()),
        (None, Some(group)) => Some(format!(":{group}")),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::build_owner_group_spec;

    #[test]
    fn owner_group_spec_formats_like_chown() {
        assert_eq!(
            build_owner_group_spec(Some("root"), Some("wheel")),
            Some("root:wheel".to_string())
        );
        assert_eq!(
            build_owner_group_spec(Some("root"), None),
            Some("root".to_string())
        );
        assert_eq!(
            build_owner_group_spec(None, Some("wheel")),
            Some(":wheel".to_string())
        );
        assert_eq!(build_owner_group_spec(None, None), None);
    }
}
