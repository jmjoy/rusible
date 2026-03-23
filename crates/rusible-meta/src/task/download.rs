use super::{TaskDetails, TaskSpec};
use rusible_template::TemplatedPath;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadTask {
    pub url: String,
    pub dest: TemplatedPath,
    #[serde(default)]
    pub force: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

impl TaskSpec for DownloadTask {
    type Details = DownloadDetails;

    fn try_from_details(details: TaskDetails) -> Option<Self::Details> {
        if let TaskDetails::Download(details) = details {
            Some(details)
        } else {
            None
        }
    }

    fn expected_task_kind() -> &'static str {
        "download"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadDetails {
    pub url: String,
    pub dest: PathBuf,
    pub downloaded: bool,
    pub bytes_written: u64,
    pub mode_changed: bool,
    pub ownership_changed: bool,
}
