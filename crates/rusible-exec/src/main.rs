use clap::Parser;
use rusible_meta::{FileState, FileTask, Task, TaskResult, TemplateTask};
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
        return Ok(TaskResult::ok(format!(
            "{} is already absent",
            path.display()
        )));
    }

    if path.is_dir() && !path.is_symlink() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }

    Ok(TaskResult::changed(format!("removed {}", path.display())))
}

fn ensure_directory(task: &FileTask) -> Result<TaskResult, Error> {
    let mut changed = false;

    if !task.path.is_dir() {
        fs::create_dir_all(&task.path)?;
        changed = true;
    }

    changed |= apply_mode(&task.path, task.mode.as_deref())?;
    changed |= apply_owner_group(&task.path, task.owner.as_deref(), task.group.as_deref())?;

    Ok(if changed {
        TaskResult::changed(format!("directory {} updated", task.path.display()))
    } else {
        TaskResult::ok(format!(
            "directory {} already in desired state",
            task.path.display()
        ))
    })
}

fn ensure_file(task: &FileTask) -> Result<TaskResult, Error> {
    let mut changed = false;

    if let Some(parent) = task.path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)?;
            changed = true;
        }
    }

    if !task.path.exists() {
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&task.path)?;
        changed = true;
    }

    if let Some(content) = &task.content {
        let current = fs::read_to_string(&task.path).ok();
        if current.as_deref() != Some(content.as_str()) {
            fs::write(&task.path, content)?;
            changed = true;
        }
    }

    changed |= apply_mode(&task.path, task.mode.as_deref())?;
    changed |= apply_owner_group(&task.path, task.owner.as_deref(), task.group.as_deref())?;

    Ok(if changed {
        TaskResult::changed(format!("file {} updated", task.path.display()))
    } else {
        TaskResult::ok(format!(
            "file {} already in desired state",
            task.path.display()
        ))
    })
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

    let mut changed = !existed;
    changed |= apply_mode(&task.path, task.mode.as_deref())?;
    changed |= apply_owner_group(&task.path, task.owner.as_deref(), task.group.as_deref())?;

    Ok(if changed {
        TaskResult::changed(format!("touched {}", task.path.display()))
    } else {
        TaskResult::ok(format!(
            "file {} already in desired state",
            task.path.display()
        ))
    })
}

fn execute_template_task(task: &TemplateTask) -> Result<TaskResult, Error> {
    let file_task = FileTask {
        path: task.dest.clone(),
        state: FileState::File,
        owner: task.owner.clone(),
        group: task.group.clone(),
        mode: task.mode.clone(),
        content: Some(task.content.clone()),
    };
    ensure_file(&file_task)
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
