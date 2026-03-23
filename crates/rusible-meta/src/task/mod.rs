pub mod command;
pub mod file;
pub mod template;

pub use command::{CommandDetails, CommandTask};
pub use file::{FileDetails, FileState, FileTask};
pub use template::{TemplateDetails, TemplateTask};

use serde::{Deserialize, Serialize};
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
}

impl TaskDetails {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::File(_) => "file",
            Self::Template(_) => "template",
            Self::Command(_) => "command",
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
