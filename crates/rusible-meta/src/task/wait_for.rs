use super::{
    TaskDataSpec, TaskDetails, TaskSpec, TaskValidationError, invalid_field, resolve_optional,
    resolve_or_default, resolve_required,
};
use crate::field::Field;
use serde::{Deserialize, Serialize};
use toml::Table;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WaitForTask {
    pub name: Field<String>,
    pub port: Field<u16>,
    pub host: Field<String>,
    pub delay_secs: Field<u64>,
    pub timeout_secs: Field<u64>,
    pub connect_timeout_secs: Field<u64>,
}

impl TaskSpec for WaitForTask {
    type Data = WaitForTaskData;
    type Details = WaitForDetails;

    fn resolve(self, context: &Table) -> Result<Self::Data, TaskValidationError> {
        Ok(WaitForTaskData {
            name: resolve_optional("wait_for", "name", self.name, context)?,
            port: resolve_required("wait_for", "port", self.port, context)?,
            host: resolve_optional("wait_for", "host", self.host, context)?,
            delay_secs: resolve_or_default(
                "wait_for",
                "delay_secs",
                self.delay_secs,
                context,
                || 0,
            )?,
            timeout_secs: resolve_required("wait_for", "timeout_secs", self.timeout_secs, context)?,
            connect_timeout_secs: resolve_or_default(
                "wait_for",
                "connect_timeout_secs",
                self.connect_timeout_secs,
                context,
                default_connect_timeout_secs,
            )?,
        })
    }

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
pub struct WaitForTaskData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default)]
    pub delay_secs: u64,
    pub timeout_secs: u64,
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
}

impl TaskDataSpec for WaitForTaskData {
    fn validate(&self) -> Result<(), TaskValidationError> {
        if self.port == 0 {
            return Err(invalid_field(
                "wait_for",
                "port",
                "port must be greater than zero",
            ));
        }

        if self.timeout_secs == 0 {
            return Err(invalid_field(
                "wait_for",
                "timeout_secs",
                "timeout_secs must be greater than zero",
            ));
        }

        if self.connect_timeout_secs == 0 {
            return Err(invalid_field(
                "wait_for",
                "connect_timeout_secs",
                "connect_timeout_secs must be greater than zero",
            ));
        }

        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_data_validate_rejects_invalid_wait_for() {
        let error = WaitForTaskData {
            name: None,
            host: None,
            port: 0,
            delay_secs: 0,
            timeout_secs: 0,
            connect_timeout_secs: 0,
        }
        .validate()
        .unwrap_err();

        assert!(matches!(error, TaskValidationError::InvalidField { .. }));
    }
}
