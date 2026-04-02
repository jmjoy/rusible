use super::{
    TaskDataSpec, TaskDetails, TaskSpec, TaskValidationError, invalid_field, resolve_optional,
    resolve_or_default, resolve_required,
};
use rusible_template::Field;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use toml::Table;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DownloadTask {
    pub name: Field<String>,
    pub url: Field<String>,
    pub dest: Field<PathBuf>,
    pub force: Field<bool>,
    pub owner: Field<String>,
    pub group: Field<String>,
    pub mode: Field<String>,
}

impl TaskSpec for DownloadTask {
    type Data = DownloadTaskData;
    type Details = DownloadDetails;

    fn resolve(self, context: &Table) -> Result<Self::Data, TaskValidationError> {
        Ok(DownloadTaskData {
            name: resolve_optional("download", "name", self.name, context)?,
            url: resolve_required("download", "url", self.url, context)?,
            dest: resolve_required("download", "dest", self.dest, context)?,
            force: resolve_or_default("download", "force", self.force, context, || false)?,
            owner: resolve_optional("download", "owner", self.owner, context)?,
            group: resolve_optional("download", "group", self.group, context)?,
            mode: resolve_optional("download", "mode", self.mode, context)?,
        })
    }

    fn try_from_details(details: TaskDetails) -> Option<Self::Details> {
        if let TaskDetails::Download(details) = details {
            Some(details)
        } else {
            None
        }
    }

    fn expected_task_kind() -> &'static str {
        "download"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadTaskData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub url: String,
    pub dest: PathBuf,
    #[serde(default)]
    pub force: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

impl TaskDataSpec for DownloadTaskData {
    fn validate(&self) -> Result<(), TaskValidationError> {
        if self.url.trim().is_empty() {
            return Err(invalid_field("download", "url", "url must not be empty"));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadDetails {
    pub url: String,
    pub dest: PathBuf,
    pub downloaded: bool,
    pub bytes_written: u64,
    pub mode_changed: bool,
    pub ownership_changed: bool,
}
