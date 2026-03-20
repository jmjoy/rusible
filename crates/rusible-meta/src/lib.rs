//! Serializable task definitions shared between the controller and executor.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Describes a task that can be executed by `rusible-exec`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Task {
    /// Ensures a file-system path reaches the requested state.
    File(FileTask),
    /// Writes rendered template content to a destination path.
    Template(TemplateTask),
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

/// Associates a task type with the structured details it returns.
pub trait TaskSpec: Into<Task> {
    /// Structured task-specific details returned for this task type.
    type Details;

    /// Converts dynamically tagged details into this task's typed details.
    fn try_from_details(details: TaskDetails) -> Option<Self::Details>;

    /// Returns the serialized task kind expected by this task type.
    fn expected_task_kind() -> &'static str;
}

/// File task parameters modelled after Ansible's file module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileTask {
    pub path: PathBuf,
    pub state: FileState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// Template task parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateTask {
    pub dest: PathBuf,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
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

impl TaskSpec for FileTask {
    type Details = FileDetails;

    fn try_from_details(details: TaskDetails) -> Option<Self::Details> {
        match details {
            TaskDetails::File(details) => Some(details),
            TaskDetails::Template(_) => None,
        }
    }

    fn expected_task_kind() -> &'static str {
        "file"
    }
}

impl TaskSpec for TemplateTask {
    type Details = TemplateDetails;

    fn try_from_details(details: TaskDetails) -> Option<Self::Details> {
        match details {
            TaskDetails::File(_) => None,
            TaskDetails::Template(details) => Some(details),
        }
    }

    fn expected_task_kind() -> &'static str {
        "template"
    }
}

/// Desired file-system state for a file task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileState {
    Absent,
    Directory,
    File,
    Touch,
}

/// Task-specific details returned by the executor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskDetails {
    /// Details produced by a file task.
    File(FileDetails),
    /// Details produced by a template task.
    Template(TemplateDetails),
}

impl TaskDetails {
    /// Returns the serialized task kind for these details.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::File(_) => "file",
            Self::Template(_) => "template",
        }
    }
}

/// Structured details for a file task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDetails {
    pub path: PathBuf,
    pub state: FileState,
    #[serde(default)]
    pub created: bool,
    #[serde(default)]
    pub removed: bool,
    #[serde(default)]
    pub content_changed: bool,
    #[serde(default)]
    pub mode_changed: bool,
    #[serde(default)]
    pub ownership_changed: bool,
}

/// Structured details for a template task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateDetails {
    pub dest: PathBuf,
    #[serde(default)]
    pub created: bool,
    #[serde(default)]
    pub content_changed: bool,
    #[serde(default)]
    pub mode_changed: bool,
    #[serde(default)]
    pub ownership_changed: bool,
}

/// High-level execution status returned by `rusible-exec` and the controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Ok,
    Changed,
    Skipped,
    Failed,
    Unreachable,
}

/// Structured task result payload.
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

    /// Attaches structured task details to the result.
    pub fn with_details(mut self, details: D) -> Self {
        self.details = Some(details);
        self
    }

    /// Converts the details payload while preserving status and message.
    pub fn try_map_details<U, E>(self, f: impl FnOnce(D) -> Result<U, E>) -> Result<TaskResult<U>, E> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_round_trips_as_json() {
        let task = Task::File(FileTask {
            path: PathBuf::from("/tmp/example"),
            state: FileState::File,
            owner: Some("root".to_string()),
            group: None,
            mode: Some("0644".to_string()),
            content: Some("hello".to_string()),
        });

        let json = serde_json::to_string(&task).unwrap();
        let decoded: Task = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, task);
    }

    #[test]
    fn task_result_with_details_round_trips_as_json() {
        let result = TaskResult::changed("updated").with_details(TaskDetails::File(FileDetails {
            path: PathBuf::from("/tmp/example"),
            state: FileState::File,
            created: true,
            removed: false,
            content_changed: true,
            mode_changed: false,
            ownership_changed: false,
        }));

        let json = serde_json::to_string(&result).unwrap();
        let decoded: TaskResult = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, result);
    }

    #[test]
    fn file_task_converts_into_task() {
        let task: Task = FileTask {
            path: PathBuf::from("/tmp/example"),
            state: FileState::Touch,
            owner: None,
            group: None,
            mode: None,
            content: None,
        }
        .into();

        assert!(matches!(task, Task::File(_)));
    }

    #[test]
    fn template_task_converts_into_task() {
        let task: Task = TemplateTask {
            dest: PathBuf::from("/tmp/example"),
            content: "hello".to_string(),
            owner: None,
            group: None,
            mode: None,
        }
        .into();

        assert!(matches!(task, Task::Template(_)));
    }

    #[test]
    fn file_task_spec_extracts_file_details() {
        let details = FileTask::try_from_details(TaskDetails::File(FileDetails {
            path: PathBuf::from("/tmp/example"),
            state: FileState::Touch,
            created: false,
            removed: false,
            content_changed: false,
            mode_changed: true,
            ownership_changed: false,
        }));

        assert!(matches!(details, Some(FileDetails { mode_changed: true, .. })));
    }
}
