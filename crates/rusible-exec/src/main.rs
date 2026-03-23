use clap::Parser;
use minijinja::{Environment, UndefinedBehavior};
use rusible_meta::{
    CommandDetails, CommandTask, FileDetails, FileState, FileTask, Task, TaskDetails,
    TaskRequest, TaskResult, TaskStatus, TemplateDetails, TemplateTask,
};
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command as ProcessCommand, ExitCode, Stdio},
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

    #[error("template rendering failed: {0}")]
    Template(#[from] minijinja::Error),

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

    let request: TaskRequest = serde_json::from_str(&stdin)?;
    execute_task(request)
}

fn execute_task(request: TaskRequest) -> Result<TaskResult, Error> {
    match request.task {
        Task::File(task) => execute_file_task(&task),
        Task::Template(task) => execute_template_task(&task, &request.context),
        Task::Command(task) => execute_command_task(&task),
    }
}

fn execute_command_task(task: &CommandTask) -> Result<TaskResult, Error> {
    let argv = resolve_command_argv(task)?;

    if let Some(path) = task.creates.as_deref() {
        if path.exists() {
            return Ok(command_task_result(
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
        if !path.exists() {
            return Ok(command_task_result(
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

    let mut command = ProcessCommand::new(&argv[0]);
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
            return Ok(command_task_result(
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
            child_stdin.write_all(stdin.as_bytes())?;
        }
    }

    let output = child.wait_with_output()?;
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
        Ok(command_task_result(
            TaskStatus::Changed,
            "command executed",
            details,
        ))
    } else {
        Ok(command_task_result(
            TaskStatus::Failed,
            format!("command failed with status {}", rc.unwrap_or(-1)),
            details,
        ))
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

    let status = ProcessCommand::new("touch").arg(&task.path).status()?;
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

fn execute_template_task(task: &TemplateTask, context: &toml::Table) -> Result<TaskResult, Error> {
    let mut changes = FileChangeSummary::default();
    let rendered = render_template(&task.content, context)?;

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
    if current.as_deref() != Some(rendered.as_str()) {
        fs::write(&task.dest, &rendered)?;
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

fn resolve_command_argv(task: &CommandTask) -> Result<Vec<String>, Error> {
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

fn file_task_result(
    status: TaskStatus, message: impl Into<String>, details: FileDetails,
) -> TaskResult {
    TaskResult {
        status,
        message: Some(message.into()),
        details: Some(TaskDetails::File(details)),
    }
}

fn template_task_result(
    status: TaskStatus, message: impl Into<String>, details: TemplateDetails,
) -> TaskResult {
    TaskResult {
        status,
        message: Some(message.into()),
        details: Some(TaskDetails::Template(details)),
    }
}

fn command_task_result(
    status: TaskStatus, message: impl Into<String>, details: CommandDetails,
) -> TaskResult {
    TaskResult {
        status,
        message: Some(message.into()),
        details: Some(TaskDetails::Command(details)),
    }
}

fn render_template(content: &str, context: &toml::Table) -> Result<String, Error> {
    let mut environment = Environment::new();
    environment.set_undefined_behavior(UndefinedBehavior::Strict);
    let template = environment.template_from_str(content)?;
    Ok(template.render(context)?)
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

    let output = ProcessCommand::new("chown").arg(&spec).arg(path).output()?;
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
    use std::{env, fs, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};

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

    #[test]
    fn resolve_command_argv_from_cmd_string() {
        let argv = resolve_command_argv(&CommandTask {
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

    #[test]
    fn execute_command_task_skips_when_creates_exists() {
        let path = unique_temp_path("creates");
        fs::write(&path, "exists").unwrap();

        let result = execute_command_task(&CommandTask {
            cmd: Some("false".to_string()),
            argv: None,
            chdir: None,
            creates: Some(path.clone()),
            removes: None,
            stdin: None,
        })
        .unwrap();

        assert_eq!(result.status, TaskStatus::Skipped);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn execute_command_task_marks_success_as_changed() {
        let result = execute_command_task(&CommandTask {
            cmd: None,
            argv: Some(vec!["true".to_string()]),
            chdir: None,
            creates: None,
            removes: None,
            stdin: None,
        })
        .unwrap();

        assert_eq!(result.status, TaskStatus::Changed);
        assert!(matches!(
            result.details,
            Some(TaskDetails::Command(CommandDetails { rc: Some(0), .. }))
        ));
    }

    #[test]
    fn execute_command_task_marks_non_zero_exit_as_failed() {
        let result = execute_command_task(&CommandTask {
            cmd: None,
            argv: Some(vec!["false".to_string()]),
            chdir: None,
            creates: None,
            removes: None,
            stdin: None,
        })
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
