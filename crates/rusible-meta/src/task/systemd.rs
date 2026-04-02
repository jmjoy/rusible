use super::{
    TaskDataSpec, TaskDetails, TaskSpec, TaskValidationError, invalid_field, resolve_optional,
    resolve_or_default, resolve_required,
};
use rusible_template::{Field, ResolveValue, ResolveValueError};
use serde::{Deserialize, Serialize};
use toml::Table;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemdTask {
    pub name: Field<String>,
    pub unit: Field<String>,
    pub daemon_reload: Field<bool>,
    pub enabled: Field<bool>,
    pub state: Field<SystemdState>,
}

impl TaskSpec for SystemdTask {
    type Data = SystemdTaskData;
    type Details = SystemdDetails;

    fn resolve(self, context: &Table) -> Result<Self::Data, TaskValidationError> {
        Ok(SystemdTaskData {
            name: resolve_optional("systemd", "name", self.name, context)?,
            unit: resolve_required("systemd", "unit", self.unit, context)?,
            daemon_reload: resolve_or_default(
                "systemd",
                "daemon_reload",
                self.daemon_reload,
                context,
                || false,
            )?,
            enabled: resolve_optional("systemd", "enabled", self.enabled, context)?,
            state: resolve_optional("systemd", "state", self.state, context)?,
        })
    }

    fn try_from_details(details: TaskDetails) -> Option<Self::Details> {
        if let TaskDetails::Systemd(details) = details {
            Some(details)
        } else {
            None
        }
    }

    fn expected_task_kind() -> &'static str {
        "systemd"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemdState {
    Started,
    Stopped,
    Restarted,
    Reloaded,
}

impl std::str::FromStr for SystemdState {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "started" => Ok(Self::Started),
            "stopped" => Ok(Self::Stopped),
            "restarted" => Ok(Self::Restarted),
            "reloaded" => Ok(Self::Reloaded),
            _ => Err("expected one of started, stopped, restarted, reloaded"),
        }
    }
}

impl ResolveValue for SystemdState {
    fn resolve_value(rendered: String) -> Result<Self, ResolveValueError> {
        rendered
            .parse()
            .map_err(|error: &'static str| ResolveValueError::Parse {
                target: Self::expected_type(),
                value: rendered,
                message: error.to_string(),
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemdTaskData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub unit: String,
    #[serde(default)]
    pub daemon_reload: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<SystemdState>,
}

impl TaskDataSpec for SystemdTaskData {
    fn validate(&self) -> Result<(), TaskValidationError> {
        if self.unit.trim().is_empty() {
            return Err(invalid_field("systemd", "unit", "unit must not be empty"));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemdDetails {
    pub unit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub changed: bool,
}
