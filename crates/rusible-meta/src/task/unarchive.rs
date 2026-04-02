use super::{
    TaskDataSpec, TaskDetails, TaskSpec, TaskValidationError, resolve_optional, resolve_required,
};
use crate::Field;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use toml::Table;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnarchiveTask {
    pub name: Field<String>,
    pub src: Field<PathBuf>,
    pub dest: Field<PathBuf>,
    pub creates: Field<PathBuf>,
}

impl TaskSpec for UnarchiveTask {
    type Data = UnarchiveTaskData;
    type Details = UnarchiveDetails;

    fn resolve(self, context: &Table) -> Result<Self::Data, TaskValidationError> {
        Ok(UnarchiveTaskData {
            name: resolve_optional("unarchive", "name", self.name, context)?,
            src: resolve_required("unarchive", "src", self.src, context)?,
            dest: resolve_required("unarchive", "dest", self.dest, context)?,
            creates: resolve_optional("unarchive", "creates", self.creates, context)?,
        })
    }

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
pub struct UnarchiveTaskData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub src: PathBuf,
    pub dest: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creates: Option<PathBuf>,
}

impl TaskDataSpec for UnarchiveTaskData {
    fn validate(&self) -> Result<(), TaskValidationError> {
        Ok(())
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
