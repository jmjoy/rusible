use super::{
    TaskDataSpec, TaskDetails, TaskSpec, TaskValidationError, invalid_field, resolve_optional,
    resolve_required,
};
use crate::field::Field;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use toml::Table;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShellTask {
    pub name: Field<String>,
    pub cmd: Field<String>,
    pub chdir: Field<PathBuf>,
    pub creates: Field<PathBuf>,
    pub removes: Field<PathBuf>,
    pub stdin: Field<String>,
}

impl TaskSpec for ShellTask {
    type Data = ShellTaskData;
    type Details = ShellDetails;

    fn resolve(self, context: &Table) -> Result<Self::Data, TaskValidationError> {
        Ok(ShellTaskData {
            name: resolve_optional("shell", "name", self.name, context)?,
            cmd: resolve_required("shell", "cmd", self.cmd, context)?,
            chdir: resolve_optional("shell", "chdir", self.chdir, context)?,
            creates: resolve_optional("shell", "creates", self.creates, context)?,
            removes: resolve_optional("shell", "removes", self.removes, context)?,
            stdin: resolve_optional("shell", "stdin", self.stdin, context)?,
        })
    }

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
pub struct ShellTaskData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
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

impl TaskDataSpec for ShellTaskData {
    fn validate(&self) -> Result<(), TaskValidationError> {
        if self.cmd.trim().is_empty() {
            return Err(invalid_field("shell", "cmd", "cmd must not be empty"));
        }

        Ok(())
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
