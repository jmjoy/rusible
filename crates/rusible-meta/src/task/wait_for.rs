use super::{TaskDetails, TaskSpec};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitForTask {
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default)]
    pub delay_secs: u64,
    pub timeout_secs: u64,
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
}

impl TaskSpec for WaitForTask {
    type Details = WaitForDetails;

    fn try_from_details(details: TaskDetails) -> Option<Self::Details> {
        if let TaskDetails::WaitFor(details) = details {
            Some(details)
        } else {
            None
        }
    }

    fn expected_task_kind() -> &'static str {
        "wait_for"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitForDetails {
    pub host: String,
    pub port: u16,
    pub attempts: u32,
    pub elapsed_ms: u64,
    pub connected: bool,
}

fn default_connect_timeout_secs() -> u64 {
    5
}