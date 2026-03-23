use super::{TaskDetails, TaskSpec};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnarchiveTask {
    pub src: PathBuf,
    pub dest: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creates: Option<PathBuf>,
}

impl TaskSpec for UnarchiveTask {
    type Details = UnarchiveDetails;

    fn try_from_details(details: TaskDetails) -> Option<Self::Details> {
        if let TaskDetails::Unarchive(details) = details {
            Some(details)
        } else {
            None
        }
    }

    fn expected_task_kind() -> &'static str {
        "unarchive"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnarchiveDetails {
    pub src: PathBuf,
    pub dest: PathBuf,
    #[serde(default)]
    pub extracted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creates: Option<PathBuf>,
}
