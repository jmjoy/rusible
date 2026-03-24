pub mod command;
pub mod copy;
pub mod download;
pub mod file;
pub mod shell;
pub mod stat;
pub mod systemd;
pub mod template;
pub mod unarchive;
pub mod user;
pub mod wait_for;

pub use command::{CommandDetails, CommandTask};
pub use copy::{CopyDetails, CopyTask};
pub use download::{DownloadDetails, DownloadTask};
pub use file::{FileDetails, FileState, FileTask};
pub use shell::{ShellDetails, ShellTask};
pub use stat::{StatDetails, StatTask};
pub use systemd::{SystemdDetails, SystemdState, SystemdTask};
pub use template::{TemplateDetails, TemplateTask};
pub use unarchive::{UnarchiveDetails, UnarchiveTask};
pub use user::{UserDetails, UserTask};
pub use wait_for::{WaitForDetails, WaitForTask};

use serde::{Deserialize, Serialize};
use std::fmt;
use toml::Table;

/// Describes a task that can be executed by `rusible-exec`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Task {
    /// Ensures a file-system path reaches the requested state.
    File(FileTask),
    /// Writes rendered template content to a destination path.
    Template(TemplateTask),
    /// Executes a command without invoking a shell.
    Command(CommandTask),
    /// Copies a file on the target host.
    Copy(CopyTask),
    /// Downloads a file from an HTTP(S) endpoint onto the target host.
    Download(DownloadTask),
    /// Executes a command through the system shell.
    Shell(ShellTask),
    /// Collects file-system metadata for a path.
    Stat(StatTask),
    /// Ensures a local user account exists.
    User(UserTask),
    /// Applies service state through systemd.
    Systemd(SystemdTask),
    /// Extracts an archive that already exists on the target host.
    Unarchive(UnarchiveTask),
    /// Waits until a TCP port becomes reachable.
    WaitFor(WaitForTask),
}

impl From<FileTask> for Task {
    fn from(task: FileTask) -> Self {
        Self::File(task)
    }
}

impl From<TemplateTask> for Task {
    fn from(task: TemplateTask) -> Self {
        Self::Template(task)
    }
}

impl From<CommandTask> for Task {
    fn from(task: CommandTask) -> Self {
        Self::Command(task)
    }
}

impl From<CopyTask> for Task {
    fn from(task: CopyTask) -> Self {
        Self::Copy(task)
    }
}

impl From<DownloadTask> for Task {
    fn from(task: DownloadTask) -> Self {
        Self::Download(task)
    }
}

impl From<ShellTask> for Task {
    fn from(task: ShellTask) -> Self {
        Self::Shell(task)
    }
}

impl From<StatTask> for Task {
    fn from(task: StatTask) -> Self {
        Self::Stat(task)
    }
}

impl From<UserTask> for Task {
    fn from(task: UserTask) -> Self {
        Self::User(task)
    }
}

impl From<SystemdTask> for Task {
    fn from(task: SystemdTask) -> Self {
        Self::Systemd(task)
    }
}

impl From<UnarchiveTask> for Task {
    fn from(task: UnarchiveTask) -> Self {
        Self::Unarchive(task)
    }
}

impl From<WaitForTask> for Task {
    fn from(task: WaitForTask) -> Self {
        Self::WaitFor(task)
    }
}

impl Task {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::File(_) => "file",
            Self::Template(_) => "template",
            Self::Command(_) => "command",
            Self::Copy(_) => "copy",
            Self::Download(_) => "download",
            Self::Shell(_) => "shell",
            Self::Stat(_) => "stat",
            Self::User(_) => "user",
            Self::Systemd(_) => "systemd",
            Self::Unarchive(_) => "unarchive",
            Self::WaitFor(_) => "wait_for",
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            Self::File(task) => task.name.as_deref(),
            Self::Template(task) => task.name.as_deref(),
            Self::Command(task) => task.name.as_deref(),
            Self::Copy(task) => task.name.as_deref(),
            Self::Download(task) => task.name.as_deref(),
            Self::Shell(task) => task.name.as_deref(),
            Self::Stat(task) => task.name.as_deref(),
            Self::User(task) => task.name.as_deref(),
            Self::Systemd(task) => task.name.as_deref(),
            Self::Unarchive(task) => task.name.as_deref(),
            Self::WaitFor(task) => task.name.as_deref(),
        }
    }

    pub fn display_name(&self) -> &str {
        self.name().unwrap_or_else(|| self.kind())
    }
}

/// Serialized task request sent from the controller to `rusible-exec`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskRequest {
    pub task: Task,
    #[serde(default, skip_serializing_if = "Table::is_empty")]
    pub context: Table,
}

impl TaskRequest {
    pub fn new(task: Task, context: Table) -> Self {
        Self { task, context }
    }
}

/// Associates a task type with the structured details it returns.
pub trait TaskSpec: Into<Task> {
    type Details;

    fn try_from_details(details: TaskDetails) -> Option<Self::Details>;

    fn expected_task_kind() -> &'static str;
}

impl TaskSpec for Task {
    type Details = TaskDetails;

    fn try_from_details(details: TaskDetails) -> Option<Self::Details> {
        Some(details)
    }

    fn expected_task_kind() -> &'static str {
        "task"
    }
}

/// Task-specific details returned by the executor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskDetails {
    File(FileDetails),
    Template(TemplateDetails),
    Command(CommandDetails),
    Copy(CopyDetails),
    Download(DownloadDetails),
    Shell(ShellDetails),
    Stat(StatDetails),
    User(UserDetails),
    Systemd(SystemdDetails),
    Unarchive(UnarchiveDetails),
    WaitFor(WaitForDetails),
}

impl TaskDetails {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::File(_) => "file",
            Self::Template(_) => "template",
            Self::Command(_) => "command",
            Self::Copy(_) => "copy",
            Self::Download(_) => "download",
            Self::Shell(_) => "shell",
            Self::Stat(_) => "stat",
            Self::User(_) => "user",
            Self::Systemd(_) => "systemd",
            Self::Unarchive(_) => "unarchive",
            Self::WaitFor(_) => "wait_for",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Ok,
    Changed,
    Skipped,
    Failed,
    Unreachable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskResult<D = TaskDetails> {
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<D>,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Changed => "changed",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
            Self::Unreachable => "unreachable",
        }
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<D> TaskResult<D> {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            status: TaskStatus::Ok,
            message: Some(message.into()),
            details: None,
        }
    }

    pub fn changed(message: impl Into<String>) -> Self {
        Self {
            status: TaskStatus::Changed,
            message: Some(message.into()),
            details: None,
        }
    }

    pub fn skipped(message: impl Into<String>) -> Self {
        Self {
            status: TaskStatus::Skipped,
            message: Some(message.into()),
            details: None,
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            status: TaskStatus::Failed,
            message: Some(message.into()),
            details: None,
        }
    }

    pub fn unreachable(message: impl Into<String>) -> Self {
        Self {
            status: TaskStatus::Unreachable,
            message: Some(message.into()),
            details: None,
        }
    }

    pub fn with_details(mut self, details: D) -> Self {
        self.details = Some(details);
        self
    }

    pub fn try_map_details<U, E>(
        self, f: impl FnOnce(D) -> Result<U, E>,
    ) -> Result<TaskResult<U>, E> {
        Ok(TaskResult {
            status: self.status,
            message: self.message,
            details: match self.details {
                Some(details) => Some(f(details)?),
                None => None,
            },
        })
    }
}
