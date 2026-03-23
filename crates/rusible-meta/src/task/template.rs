use super::{TaskDetails, TaskSpec};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

impl TaskSpec for TemplateTask {
    type Details = TemplateDetails;

    fn try_from_details(details: TaskDetails) -> Option<Self::Details> {
        if let TaskDetails::Template(details) = details {
            Some(details)
        } else {
            None
        }
    }

    fn expected_task_kind() -> &'static str {
        "template"
    }
}

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