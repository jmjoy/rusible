use super::{
    TaskDataSpec, TaskDetails, TaskSpec, TaskValidationError, invalid_field, resolve_optional,
    resolve_required,
};
use crate::field::{Field, ResolveValue, ResolveValueError};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use toml::Table;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileTask {
    pub name: Field<String>,
    pub path: Field<PathBuf>,
    pub state: Field<FileState>,
    pub owner: Field<String>,
    pub group: Field<String>,
    pub mode: Field<String>,
    pub content: Field<String>,
}

impl TaskSpec for FileTask {
    type Data = FileTaskData;
    type Details = FileDetails;

    fn resolve(self, context: &Table) -> Result<Self::Data, TaskValidationError> {
        Ok(FileTaskData {
            name: resolve_optional("file", "name", self.name, context)?,
            path: resolve_required("file", "path", self.path, context)?,
            state: resolve_required("file", "state", self.state, context)?,
            owner: resolve_optional("file", "owner", self.owner, context)?,
            group: resolve_optional("file", "group", self.group, context)?,
            mode: resolve_optional("file", "mode", self.mode, context)?,
            content: resolve_optional("file", "content", self.content, context)?,
        })
    }

    fn try_from_details(details: TaskDetails) -> Option<Self::Details> {
        if let TaskDetails::File(details) = details {
            Some(details)
        } else {
            None
        }
    }

    fn expected_task_kind() -> &'static str {
        "file"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileState {
    Absent,
    Directory,
    File,
    Touch,
}

impl std::str::FromStr for FileState {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "absent" => Ok(Self::Absent),
            "directory" => Ok(Self::Directory),
            "file" => Ok(Self::File),
            "touch" => Ok(Self::Touch),
            _ => Err("expected one of absent, directory, file, touch"),
        }
    }
}

impl ResolveValue for FileState {
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
pub struct FileTaskData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub path: PathBuf,
    pub state: FileState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

impl TaskDataSpec for FileTaskData {
    fn validate(&self) -> Result<(), TaskValidationError> {
        if matches!(self.state, FileState::Directory | FileState::Absent) && self.content.is_some()
        {
            return Err(invalid_field(
                "file",
                "content",
                "content is only supported when state is `file`",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDetails {
    pub path: PathBuf,
    pub state: FileState,
    #[serde(default)]
    pub created: bool,
    #[serde(default)]
    pub removed: bool,
    #[serde(default)]
    pub content_changed: bool,
    #[serde(default)]
    pub mode_changed: bool,
    #[serde(default)]
    pub ownership_changed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_task_spec_extracts_file_details() {
        let details = FileTask::try_from_details(TaskDetails::File(FileDetails {
            path: PathBuf::from("/tmp/example"),
            state: FileState::Touch,
            created: false,
            removed: false,
            content_changed: false,
            mode_changed: true,
            ownership_changed: false,
        }));

        assert!(matches!(
            details,
            Some(FileDetails {
                mode_changed: true,
                ..
            })
        ));
    }
}
