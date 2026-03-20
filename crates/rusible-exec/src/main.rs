use clap::Parser;
use rusible_meta::{
    FileDetails, FileState, FileTask, Task, TaskDetails, TaskResult, TaskStatus,
    TemplateDetails, TemplateTask,
};
use std::{
    fs::{self, OpenOptions},
    io::{self, Read},
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, ExitCode},
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

    #[error("command `{program}` failed with status {status}: {stderr}")]
    CommandFailed {
        program: String,
        status: i32,
        stderr: String,
    },
}

#[derive(Debug, Default)]
struct FileChangeSummary {
    created: bool,
    removed: bool,
    content_changed: bool,
    mode_changed: bool,
    ownership_changed: bool,
}

impl FileChangeSummary {
    fn any(&self) -> bool {
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

    fn into_template_details(self, dest: &Path) -> TemplateDetails {
        TemplateDetails {
            dest: dest.to_path_buf(),
            created: self.created,
            content_changed: self.content_changed,
            mode_changed: self.mode_changed,
            ownership_changed: self.ownership_changed,
        }
    }
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

    let task: Task = serde_json::from_str(&stdin)?;
    execute_task(task)
}

fn execute_task(task: Task) -> Result<TaskResult, Error> {
    match task {
        Task::File(task) => execute_file_task(&task),
        Task::Template(task) => execute_template_task(&task),
    }
}

fn execute_file_task(task: &FileTask) -> Result<TaskResult, Error> {
    match task.state {
        FileState::Absent => ensure_absent(&task.path),
        FileState::Directory => ensure_directory(task),
        FileState::File => ensure_file(task),
        FileState::Touch => ensure_touch(task),
    }
}

fn ensure_absent(path: &Path) -> Result<TaskResult, Error> {
    if !path.exists() {
        return Ok(file_task_result(
            TaskStatus::Ok,
            format!("{} is already absent", path.display()),
            FileChangeSummary::default().into_file_details(path, FileState::Absent),
        ));
    }

    if path.is_dir() && !path.is_symlink() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }

    Ok(file_task_result(
        TaskStatus::Changed,
        format!("removed {}", path.display()),
        FileChangeSummary {
            removed: true,
            ..FileChangeSummary::default()
        }
        .into_file_details(path, FileState::Absent),
    ))
}

fn ensure_directory(task: &FileTask) -> Result<TaskResult, Error> {
    let mut changes = FileChangeSummary::default();

    if !task.path.is_dir() {
        fs::create_dir_all(&task.path)?;
        changes.created = true;
    }

    changes.mode_changed = apply_mode(&task.path, task.mode.as_deref())?;
    changes.ownership_changed =
        apply_owner_group(&task.path, task.owner.as_deref(), task.group.as_deref())?;

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

    Ok(file_task_result(
        status,
        message,
        changes.into_file_details(&task.path, FileState::Directory),
    ))
}

fn ensure_file(task: &FileTask) -> Result<TaskResult, Error> {
    let mut changes = FileChangeSummary::default();

    if let Some(parent) = task.path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }

    if !task.path.exists() {
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&task.path)?;
        changes.created = true;
    }

    if let Some(content) = &task.content {
        let current = fs::read_to_string(&task.path).ok();
        if current.as_deref() != Some(content.as_str()) {
            fs::write(&task.path, content)?;
            changes.content_changed = true;
        }
    }

    changes.mode_changed = apply_mode(&task.path, task.mode.as_deref())?;
    changes.ownership_changed =
        apply_owner_group(&task.path, task.owner.as_deref(), task.group.as_deref())?;

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

    Ok(file_task_result(
        status,
        message,
        changes.into_file_details(&task.path, FileState::File),
    ))
}

fn ensure_touch(task: &FileTask) -> Result<TaskResult, Error> {
    let existed = task.path.exists();

    if let Some(parent) = task.path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }

    let status = Command::new("touch").arg(&task.path).status()?;
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
    changes.mode_changed = apply_mode(&task.path, task.mode.as_deref())?;
    changes.ownership_changed =
        apply_owner_group(&task.path, task.owner.as_deref(), task.group.as_deref())?;

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

    Ok(file_task_result(
        status,
        message,
        changes.into_file_details(&task.path, FileState::Touch),
    ))
}

fn execute_template_task(task: &TemplateTask) -> Result<TaskResult, Error> {
    let mut changes = FileChangeSummary::default();

    if let Some(parent) = task.dest.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }

    if !task.dest.exists() {
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&task.dest)?;
        changes.created = true;
    }

    let current = fs::read_to_string(&task.dest).ok();
    if current.as_deref() != Some(task.content.as_str()) {
        fs::write(&task.dest, &task.content)?;
        changes.content_changed = true;
    }

    changes.mode_changed = apply_mode(&task.dest, task.mode.as_deref())?;
    changes.ownership_changed =
        apply_owner_group(&task.dest, task.owner.as_deref(), task.group.as_deref())?;

    let status = if changes.any() {
        TaskStatus::Changed
    } else {
        TaskStatus::Ok
    };
    let message = if changes.any() {
        format!("template {} updated", task.dest.display())
    } else {
        format!("template {} already in desired state", task.dest.display())
    };

    Ok(template_task_result(
        status,
        message,
        changes.into_template_details(&task.dest),
    ))
}

fn file_task_result(status: TaskStatus, message: impl Into<String>, details: FileDetails) -> TaskResult {
    TaskResult {
        status,
        message: Some(message.into()),
        details: Some(TaskDetails::File(details)),
    }
}

fn template_task_result(
    status: TaskStatus,
    message: impl Into<String>,
    details: TemplateDetails,
) -> TaskResult {
    TaskResult {
        status,
        message: Some(message.into()),
        details: Some(TaskDetails::Template(details)),
    }
}

fn apply_mode(path: &Path, mode: Option<&str>) -> Result<bool, Error> {
    let Some(mode) = mode else {
        return Ok(false);
    };

    let desired = u32::from_str_radix(mode, 8).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid octal mode `{mode}`: {error}"),
        )
    })?;

    let metadata = fs::metadata(path)?;
    let current = metadata.permissions().mode() & 0o7777;
    if current == desired {
        return Ok(false);
    }

    let mut permissions = metadata.permissions();
    permissions.set_mode(desired);
    fs::set_permissions(path, permissions)?;
    Ok(true)
}

fn apply_owner_group(path: &Path, owner: Option<&str>, group: Option<&str>) -> Result<bool, Error> {
    let Some(spec) = build_owner_group_spec(owner, group) else {
        return Ok(false);
    };

    let output = Command::new("chown").arg(&spec).arg(path).output()?;
    if !output.status.success() {
        return Err(Error::CommandFailed {
            program: "chown".to_string(),
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    Ok(true)
}

fn build_owner_group_spec(owner: Option<&str>, group: Option<&str>) -> Option<String> {
    match (owner, group) {
        (Some(owner), Some(group)) => Some(format!("{owner}:{group}")),
        (Some(owner), None) => Some(owner.to_string()),
        (None, Some(group)) => Some(format!(":{group}")),
        (None, None) => None,
    }
}

fn print_result(result: &TaskResult) {
    println!(
        "{}",
        serde_json::to_string(result).expect("serializing task result")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

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
