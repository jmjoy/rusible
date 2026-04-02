use super::{
    TaskDataSpec, TaskDetails, TaskSpec, TaskValidationError, invalid_field, resolve_optional,
    resolve_or_default, resolve_required,
};
use crate::Field;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use toml::Table;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UserTask {
    pub name: Field<String>,
    pub username: Field<String>,
    pub system: Field<bool>,
    pub create_home: Field<bool>,
    pub shell: Field<PathBuf>,
    pub home: Field<PathBuf>,
}

impl TaskSpec for UserTask {
    type Data = UserTaskData;
    type Details = UserDetails;

    fn resolve(self, context: &Table) -> Result<Self::Data, TaskValidationError> {
        Ok(UserTaskData {
            name: resolve_optional("user", "name", self.name, context)?,
            username: resolve_required("user", "username", self.username, context)?,
            system: resolve_or_default("user", "system", self.system, context, || false)?,
            create_home: resolve_or_default(
                "user",
                "create_home",
                self.create_home,
                context,
                || false,
            )?,
            shell: resolve_optional("user", "shell", self.shell, context)?,
            home: resolve_optional("user", "home", self.home, context)?,
        })
    }

    fn try_from_details(details: TaskDetails) -> Option<Self::Details> {
        if let TaskDetails::User(details) = details {
            Some(details)
        } else {
            None
        }
    }

    fn expected_task_kind() -> &'static str {
        "user"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserTaskData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub username: String,
    #[serde(default)]
    pub system: bool,
    #[serde(default)]
    pub create_home: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<PathBuf>,
}

impl TaskDataSpec for UserTaskData {
    fn validate(&self) -> Result<(), TaskValidationError> {
        if self.username.trim().is_empty() {
            return Err(invalid_field(
                "user",
                "username",
                "username must not be empty",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserDetails {
    pub name: String,
    pub exists: bool,
    #[serde(default)]
    pub created: bool,
}
