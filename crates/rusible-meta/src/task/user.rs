use super::{TaskDetails, TaskSpec};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserTask {
    pub name: String,
    #[serde(default)]
    pub system: bool,
    #[serde(default)]
    pub create_home: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<PathBuf>,
}

impl TaskSpec for UserTask {
    type Details = UserDetails;

    fn try_from_details(details: TaskDetails) -> Option<Self::Details> {
        if let TaskDetails::User(details) = details {
            Some(details)
        } else {
            None
        }
    }

    fn expected_task_kind() -> &'static str {
        "user"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserDetails {
    pub name: String,
    pub exists: bool,
    #[serde(default)]
    pub created: bool,
}