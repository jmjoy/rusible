use super::{TaskDetails, TaskSpec};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

impl TaskSpec for FileTask {
    type Details = FileDetails;

    fn try_from_details(details: TaskDetails) -> Option<Self::Details> {
        if let TaskDetails::File(details) = details {
            Some(details)
        } else {
            None
        }
    }

    fn expected_task_kind() -> &'static str {
        "file"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileState {
    Absent,
    Directory,
    File,
    Touch,
}

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