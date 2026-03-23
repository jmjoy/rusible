use super::{TaskDetails, TaskSpec};
use rusible_template::TemplatedPath;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyTask {
    pub src: TemplatedPath,
    pub dest: TemplatedPath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

impl TaskSpec for CopyTask {
    type Details = CopyDetails;

    fn try_from_details(details: TaskDetails) -> Option<Self::Details> {
        if let TaskDetails::Copy(details) = details {
            Some(details)
        } else {
            None
        }
    }

    fn expected_task_kind() -> &'static str {
        "copy"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyDetails {
    pub src: PathBuf,
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
