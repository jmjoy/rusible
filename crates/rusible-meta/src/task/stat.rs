use super::{
    TaskDataSpec, TaskDetails, TaskSpec, TaskValidationError, resolve_optional, resolve_required,
};
use crate::Field;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use toml::Table;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StatTask {
    pub name: Field<String>,
    pub path: Field<PathBuf>,
}

impl TaskSpec for StatTask {
    type Data = StatTaskData;
    type Details = StatDetails;

    fn resolve(self, context: &Table) -> Result<Self::Data, TaskValidationError> {
        Ok(StatTaskData {
            name: resolve_optional("stat", "name", self.name, context)?,
            path: resolve_required("stat", "path", self.path, context)?,
        })
    }

    fn try_from_details(details: TaskDetails) -> Option<Self::Details> {
        if let TaskDetails::Stat(details) = details {
            Some(details)
        } else {
            None
        }
    }

    fn expected_task_kind() -> &'static str {
        "stat"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatTaskData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub path: PathBuf,
}

impl TaskDataSpec for StatTaskData {
    fn validate(&self) -> Result<(), TaskValidationError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatDetails {
    pub path: PathBuf,
    pub exists: bool,
    #[serde(default)]
    pub is_file: bool,
    #[serde(default)]
    pub is_dir: bool,
    #[serde(default)]
    pub is_symlink: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}
