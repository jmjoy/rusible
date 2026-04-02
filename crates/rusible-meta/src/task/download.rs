use super::{
    TaskDataSpec, TaskDetails, TaskSpec, TaskValidationError, invalid_field, resolve_optional,
    resolve_or_default, resolve_required,
};
use crate::Field;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use toml::Table;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DownloadTask {
    pub name: Field<String>,
    pub url: Field<Url>,
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
        let url = resolve_required("download", "url", self.url, context)?;
        validate_download_url(&url)?;

        Ok(DownloadTaskData {
            name: resolve_optional("download", "name", self.name, context)?,
            url,
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
    pub url: Url,
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
        validate_download_url(&self.url)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadDetails {
    pub url: Url,
    pub dest: PathBuf,
    pub downloaded: bool,
    pub bytes_written: u64,
    pub mode_changed: bool,
    pub ownership_changed: bool,
}

fn validate_download_url(url: &Url) -> Result<(), TaskValidationError> {
    match url.scheme() {
        "http" | "https" => Ok(()),
        scheme => Err(invalid_field(
            "download",
            "url",
            format!("unsupported url scheme `{scheme}`; expected http or https"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_task_resolves_templated_url() {
        let context = toml::toml! {
            download = { host = "example.com", path = "artifact.tar.gz" }
        };

        let resolved = DownloadTask {
            name: Field::Nil,
            url: Field::tpl("https://{{ download.host }}/{{ download.path }}"),
            dest: PathBuf::from("/tmp/artifact.tar.gz").into(),
            force: Field::Nil,
            owner: Field::Nil,
            group: Field::Nil,
            mode: Field::Nil,
        }
        .resolve(&context)
        .unwrap();

        assert_eq!(
            resolved.url,
            Url::parse("https://example.com/artifact.tar.gz").unwrap()
        );
    }

    #[test]
    fn download_task_rejects_non_http_scheme() {
        let error = DownloadTask {
            name: Field::Nil,
            url: Field::val(Url::parse("ftp://example.com/archive.tar.gz").unwrap()),
            dest: PathBuf::from("/tmp/archive.tar.gz").into(),
            force: Field::Nil,
            owner: Field::Nil,
            group: Field::Nil,
            mode: Field::Nil,
        }
        .resolve(&Table::new())
        .unwrap_err();

        assert!(matches!(
            error,
            TaskValidationError::InvalidField {
                task_kind: "download",
                ..
            }
        ));
    }

    #[test]
    fn download_task_rejects_invalid_url_template_output() {
        let context = toml::toml! {
            download = { value = "not a url" }
        };

        let error = DownloadTask {
            name: Field::Nil,
            url: Field::tpl("{{ download.value }}"),
            dest: PathBuf::from("/tmp/archive.tar.gz").into(),
            force: Field::Nil,
            owner: Field::Nil,
            group: Field::Nil,
            mode: Field::Nil,
        }
        .resolve(&context)
        .unwrap_err();

        assert!(matches!(
            error,
            TaskValidationError::InvalidField {
                task_kind: "download",
                ..
            }
        ));
    }
}
