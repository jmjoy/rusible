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

/// Desired file-system state for a file task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileState {
    Absent,
    Directory,
    File,
    Touch,
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
pub struct TaskResult {
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl TaskResult {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            status: TaskStatus::Ok,
            message: Some(message.into()),
        }
    }

    pub fn changed(message: impl Into<String>) -> Self {
        Self {
            status: TaskStatus::Changed,
            message: Some(message.into()),
        }
    }

    pub fn skipped(message: impl Into<String>) -> Self {
        Self {
            status: TaskStatus::Skipped,
            message: Some(message.into()),
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            status: TaskStatus::Failed,
            message: Some(message.into()),
        }
    }

    pub fn unreachable(message: impl Into<String>) -> Self {
        Self {
            status: TaskStatus::Unreachable,
            message: Some(message.into()),
        }
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
}
