use super::{
    TaskDataSpec, TaskDetails, TaskSpec, TaskValidationError, invalid_field, invalid_task,
    missing_field, resolve_optional,
};
use crate::Field;
use serde::{Deserialize, Serialize};
use shlex::split;
use std::path::PathBuf;
use toml::Table;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommandTask {
    pub name: Field<String>,
    pub cmd: Field<String>,
    pub argv: Vec<Field<String>>,
    pub chdir: Field<PathBuf>,
    pub creates: Field<PathBuf>,
    pub removes: Field<PathBuf>,
    pub stdin: Field<String>,
}

impl TaskSpec for CommandTask {
    type Data = CommandTaskData;
    type Details = CommandDetails;

    fn resolve(self, context: &Table) -> Result<Self::Data, TaskValidationError> {
        let cmd = resolve_optional("command", "cmd", self.cmd, context)?;
        let argv = resolve_argv_values(self.argv, context)?;

        let argv = match (cmd, argv.is_empty()) {
            (Some(_), false) => {
                return Err(invalid_task(
                    "command",
                    "command task accepts either cmd or argv, not both",
                ));
            }
            (Some(cmd), true) => split(&cmd).ok_or_else(|| {
                invalid_field("command", "cmd", format!("invalid command string: {cmd}"))
            })?,
            (None, false) => argv,
            (None, true) => {
                return Err(invalid_task(
                    "command",
                    "command task requires either cmd or argv",
                ));
            }
        };

        if argv.is_empty() {
            return Err(invalid_task("command", "command argv must not be empty"));
        }

        Ok(CommandTaskData {
            name: resolve_optional("command", "name", self.name, context)?,
            argv,
            chdir: resolve_optional("command", "chdir", self.chdir, context)?,
            creates: resolve_optional("command", "creates", self.creates, context)?,
            removes: resolve_optional("command", "removes", self.removes, context)?,
            stdin: resolve_optional("command", "stdin", self.stdin, context)?,
        })
    }

    fn try_from_details(details: TaskDetails) -> Option<Self::Details> {
        if let TaskDetails::Command(details) = details {
            Some(details)
        } else {
            None
        }
    }

    fn expected_task_kind() -> &'static str {
        "command"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandTaskData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub argv: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chdir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creates: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removes: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin: Option<String>,
}

impl TaskDataSpec for CommandTaskData {
    fn validate(&self) -> Result<(), TaskValidationError> {
        if self.argv.is_empty() {
            return Err(invalid_task("command", "command argv must not be empty"));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandDetails {
    pub cmd: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chdir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rc: Option<i32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stdout: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stderr: String,
}

fn resolve_argv_values(
    argv: Vec<Field<String>>, context: &Table,
) -> Result<Vec<String>, TaskValidationError> {
    argv.into_iter()
        .enumerate()
        .map(|(index, value)| {
            value
                .resolve(context)
                .map_err(|error| {
                    invalid_field("command", format!("argv[{index}]"), error.to_string())
                })?
                .ok_or_else(|| missing_field("command", "argv[]"))
        })
        .collect()
}
