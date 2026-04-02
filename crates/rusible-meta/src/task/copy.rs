use super::{
    TaskDataSpec, TaskDetails, TaskSpec, TaskValidationError, resolve_optional, resolve_required,
};
use rusible_template::Field;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use toml::Table;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CopyTask {
    pub name: Field<String>,
    pub src: Field<PathBuf>,
    pub dest: Field<PathBuf>,
    pub owner: Field<String>,
    pub group: Field<String>,
    pub mode: Field<String>,
}

impl TaskSpec for CopyTask {
    type Data = CopyTaskData;
    type Details = CopyDetails;

    fn resolve(self, context: &Table) -> Result<Self::Data, TaskValidationError> {
        Ok(CopyTaskData {
            name: resolve_optional("copy", "name", self.name, context)?,
            src: resolve_required("copy", "src", self.src, context)?,
            dest: resolve_required("copy", "dest", self.dest, context)?,
            owner: resolve_optional("copy", "owner", self.owner, context)?,
            group: resolve_optional("copy", "group", self.group, context)?,
            mode: resolve_optional("copy", "mode", self.mode, context)?,
        })
    }

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
pub struct CopyTaskData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub src: PathBuf,
    pub dest: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

impl TaskDataSpec for CopyTaskData {
    fn validate(&self) -> Result<(), TaskValidationError> {
        Ok(())
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
