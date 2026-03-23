use super::{TaskDetails, TaskSpec};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellTask {
    pub cmd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chdir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creates: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removes: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin: Option<String>,
}

impl TaskSpec for ShellTask {
    type Details = ShellDetails;

    fn try_from_details(details: TaskDetails) -> Option<Self::Details> {
        if let TaskDetails::Shell(details) = details {
            Some(details)
        } else {
            None
        }
    }

    fn expected_task_kind() -> &'static str {
        "shell"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellDetails {
    pub cmd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chdir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rc: Option<i32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stdout: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stderr: String,
}