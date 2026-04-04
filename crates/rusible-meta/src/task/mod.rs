//! Task definitions and transport models grouped by task kind.

pub mod command;
pub mod copy;
pub mod download;
pub mod facts;
pub mod file;
pub mod shell;
pub mod stat;
pub mod systemd;
pub mod unarchive;
pub mod user;
pub mod wait_for;

use self::{
    command::{CommandDetails, CommandTask, CommandTaskData},
    copy::{CopyDetails, CopyTask, CopyTaskData},
    download::{DownloadDetails, DownloadTask, DownloadTaskData},
    facts::{FactsDetails, FactsTask, FactsTaskData},
    file::{FileDetails, FileTask, FileTaskData},
    shell::{ShellDetails, ShellTask, ShellTaskData},
    stat::{StatDetails, StatTask, StatTaskData},
    systemd::{SystemdDetails, SystemdTask, SystemdTaskData},
    unarchive::{UnarchiveDetails, UnarchiveTask, UnarchiveTaskData},
    user::{UserDetails, UserTask, UserTaskData},
    wait_for::{WaitForDetails, WaitForTask, WaitForTaskData},
};
use crate::field::{Field, ResolveValue};
use serde::{Deserialize, Serialize};
use std::fmt;
use toml::Table;

/// Describes a user-facing task definition resolved on the controller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Task {
    File(FileTask),
    /// Executes a command without invoking a shell.
    Command(CommandTask),
    /// Copies a file on the target host.
    Copy(CopyTask),
    /// Downloads a file from an HTTP(S) endpoint onto the target host.
    Download(DownloadTask),
    /// Collects runtime facts from the target host.
    Facts(FactsTask),
    /// Executes a command through the system shell.
    Shell(ShellTask),
    /// Collects file-system metadata for a path.
    Stat(StatTask),
    /// Ensures a local user account exists.
    User(UserTask),
    /// Applies service state through systemd.
    Systemd(SystemdTask),
    /// Extracts an archive that already exists on the target host.
    Unarchive(UnarchiveTask),
    /// Waits until a TCP port becomes reachable.
    WaitFor(WaitForTask),
}

impl Default for Task {
    fn default() -> Self {
        Self::File(FileTask::default())
    }
}

/// Describes a fully-resolved task that can be executed by `rusible-exec`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskData {
    /// Ensures a file-system path reaches the requested state.
    File(FileTaskData),
    /// Executes a command without invoking a shell.
    Command(CommandTaskData),
    /// Copies a file on the target host.
    Copy(CopyTaskData),
    /// Downloads a file from an HTTP(S) endpoint onto the target host.
    Download(DownloadTaskData),
    /// Collects runtime facts from the target host.
    Facts(FactsTaskData),
    /// Executes a command through the system shell.
    Shell(ShellTaskData),
    /// Collects file-system metadata for a path.
    Stat(StatTaskData),
    /// Ensures a local user account exists.
    User(UserTaskData),
    /// Applies service state through systemd.
    Systemd(SystemdTaskData),
    /// Extracts an archive that already exists on the target host.
    Unarchive(UnarchiveTaskData),
    /// Waits until a TCP port becomes reachable.
    WaitFor(WaitForTaskData),
}

impl From<FileTask> for Task {
    fn from(task: FileTask) -> Self {
        Self::File(task)
    }
}

impl From<CommandTask> for Task {
    fn from(task: CommandTask) -> Self {
        Self::Command(task)
    }
}

impl From<CopyTask> for Task {
    fn from(task: CopyTask) -> Self {
        Self::Copy(task)
    }
}

impl From<DownloadTask> for Task {
    fn from(task: DownloadTask) -> Self {
        Self::Download(task)
    }
}

impl From<FactsTask> for Task {
    fn from(task: FactsTask) -> Self {
        Self::Facts(task)
    }
}

impl From<ShellTask> for Task {
    fn from(task: ShellTask) -> Self {
        Self::Shell(task)
    }
}

impl From<StatTask> for Task {
    fn from(task: StatTask) -> Self {
        Self::Stat(task)
    }
}

impl From<UserTask> for Task {
    fn from(task: UserTask) -> Self {
        Self::User(task)
    }
}

impl From<SystemdTask> for Task {
    fn from(task: SystemdTask) -> Self {
        Self::Systemd(task)
    }
}

impl From<UnarchiveTask> for Task {
    fn from(task: UnarchiveTask) -> Self {
        Self::Unarchive(task)
    }
}

impl From<WaitForTask> for Task {
    fn from(task: WaitForTask) -> Self {
        Self::WaitFor(task)
    }
}

impl Task {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::File(_) => "file",
            Self::Command(_) => "command",
            Self::Copy(_) => "copy",
            Self::Download(_) => "download",
            Self::Facts(_) => "facts",
            Self::Shell(_) => "shell",
            Self::Stat(_) => "stat",
            Self::User(_) => "user",
            Self::Systemd(_) => "systemd",
            Self::Unarchive(_) => "unarchive",
            Self::WaitFor(_) => "wait_for",
        }
    }

    pub fn resolve(self, context: &Table) -> Result<TaskData, TaskValidationError> {
        match self {
            Self::File(task) => task.resolve(context).map(TaskData::File),
            Self::Command(task) => task.resolve(context).map(TaskData::Command),
            Self::Copy(task) => task.resolve(context).map(TaskData::Copy),
            Self::Download(task) => task.resolve(context).map(TaskData::Download),
            Self::Facts(task) => task.resolve(context).map(TaskData::Facts),
            Self::Shell(task) => task.resolve(context).map(TaskData::Shell),
            Self::Stat(task) => task.resolve(context).map(TaskData::Stat),
            Self::User(task) => task.resolve(context).map(TaskData::User),
            Self::Systemd(task) => task.resolve(context).map(TaskData::Systemd),
            Self::Unarchive(task) => task.resolve(context).map(TaskData::Unarchive),
            Self::WaitFor(task) => task.resolve(context).map(TaskData::WaitFor),
        }
    }
}

impl From<FileTaskData> for TaskData {
    fn from(task: FileTaskData) -> Self {
        Self::File(task)
    }
}

impl From<CommandTaskData> for TaskData {
    fn from(task: CommandTaskData) -> Self {
        Self::Command(task)
    }
}

impl From<CopyTaskData> for TaskData {
    fn from(task: CopyTaskData) -> Self {
        Self::Copy(task)
    }
}

impl From<DownloadTaskData> for TaskData {
    fn from(task: DownloadTaskData) -> Self {
        Self::Download(task)
    }
}

impl From<FactsTaskData> for TaskData {
    fn from(task: FactsTaskData) -> Self {
        Self::Facts(task)
    }
}

impl From<ShellTaskData> for TaskData {
    fn from(task: ShellTaskData) -> Self {
        Self::Shell(task)
    }
}

impl From<StatTaskData> for TaskData {
    fn from(task: StatTaskData) -> Self {
        Self::Stat(task)
    }
}

impl From<UserTaskData> for TaskData {
    fn from(task: UserTaskData) -> Self {
        Self::User(task)
    }
}

impl From<SystemdTaskData> for TaskData {
    fn from(task: SystemdTaskData) -> Self {
        Self::Systemd(task)
    }
}

impl From<UnarchiveTaskData> for TaskData {
    fn from(task: UnarchiveTaskData) -> Self {
        Self::Unarchive(task)
    }
}

impl From<WaitForTaskData> for TaskData {
    fn from(task: WaitForTaskData) -> Self {
        Self::WaitFor(task)
    }
}

impl TaskData {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::File(_) => "file",
            Self::Command(_) => "command",
            Self::Copy(_) => "copy",
            Self::Download(_) => "download",
            Self::Facts(_) => "facts",
            Self::Shell(_) => "shell",
            Self::Stat(_) => "stat",
            Self::User(_) => "user",
            Self::Systemd(_) => "systemd",
            Self::Unarchive(_) => "unarchive",
            Self::WaitFor(_) => "wait_for",
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            Self::File(task) => task.name.as_deref(),
            Self::Command(task) => task.name.as_deref(),
            Self::Copy(task) => task.name.as_deref(),
            Self::Download(task) => task.name.as_deref(),
            Self::Facts(task) => task.name.as_deref(),
            Self::Shell(task) => task.name.as_deref(),
            Self::Stat(task) => task.name.as_deref(),
            Self::User(task) => task.name.as_deref(),
            Self::Systemd(task) => task.name.as_deref(),
            Self::Unarchive(task) => task.name.as_deref(),
            Self::WaitFor(task) => task.name.as_deref(),
        }
    }

    pub fn display_name(&self) -> &str {
        self.name().unwrap_or_else(|| self.kind())
    }

    pub fn validate(&self) -> Result<(), TaskValidationError> {
        match self {
            Self::File(task) => task.validate(),
            Self::Command(task) => task.validate(),
            Self::Copy(task) => task.validate(),
            Self::Download(task) => task.validate(),
            Self::Facts(task) => task.validate(),
            Self::Shell(task) => task.validate(),
            Self::Stat(task) => task.validate(),
            Self::User(task) => task.validate(),
            Self::Systemd(task) => task.validate(),
            Self::Unarchive(task) => task.validate(),
            Self::WaitFor(task) => task.validate(),
        }
    }
}

/// Serialized task request sent from the controller to `rusible-exec`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRequest {
    pub task: TaskData,
}

impl TaskRequest {
    pub fn new(task: impl Into<TaskData>) -> Self {
        Self { task: task.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TaskValidationError {
    #[error("{task_kind} task field `{field}` is required")]
    MissingField {
        task_kind: &'static str,
        field: &'static str,
    },

    #[error("{task_kind} task field `{field}` is invalid: {message}")]
    InvalidField {
        task_kind: &'static str,
        field: String,
        message: String,
    },

    #[error("{task_kind} task is invalid: {message}")]
    InvalidTask {
        task_kind: &'static str,
        message: String,
    },
}

pub trait TaskDataSpec: Into<TaskData> {
    fn validate(&self) -> Result<(), TaskValidationError>;
}

/// Associates a user-side task with its transport data and structured details.
pub trait TaskSpec: Into<Task> {
    type Data: TaskDataSpec;
    type Details;

    fn resolve(self, context: &Table) -> Result<Self::Data, TaskValidationError>;

    fn try_from_details(details: TaskDetails) -> Option<Self::Details>;

    fn expected_task_kind() -> &'static str;
}

impl TaskSpec for Task {
    type Data = TaskData;
    type Details = TaskDetails;

    fn resolve(self, context: &Table) -> Result<Self::Data, TaskValidationError> {
        self.resolve(context)
    }

    fn try_from_details(details: TaskDetails) -> Option<Self::Details> {
        Some(details)
    }

    fn expected_task_kind() -> &'static str {
        "task"
    }
}

impl TaskDataSpec for TaskData {
    fn validate(&self) -> Result<(), TaskValidationError> {
        self.validate()
    }
}

/// Task-specific details returned by the executor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskDetails {
    File(FileDetails),
    Command(CommandDetails),
    Copy(CopyDetails),
    Download(DownloadDetails),
    Facts(FactsDetails),
    Shell(ShellDetails),
    Stat(StatDetails),
    User(UserDetails),
    Systemd(SystemdDetails),
    Unarchive(UnarchiveDetails),
    WaitFor(WaitForDetails),
}

impl TaskDetails {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::File(_) => "file",
            Self::Command(_) => "command",
            Self::Copy(_) => "copy",
            Self::Download(_) => "download",
            Self::Facts(_) => "facts",
            Self::Shell(_) => "shell",
            Self::Stat(_) => "stat",
            Self::User(_) => "user",
            Self::Systemd(_) => "systemd",
            Self::Unarchive(_) => "unarchive",
            Self::WaitFor(_) => "wait_for",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Ok,
    Changed,
    Skipped,
    Failed,
    Unreachable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskResult<D = TaskDetails> {
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<D>,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Changed => "changed",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
            Self::Unreachable => "unreachable",
        }
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

pub(crate) fn missing_field(task_kind: &'static str, field: &'static str) -> TaskValidationError {
    TaskValidationError::MissingField { task_kind, field }
}

pub(crate) fn invalid_field(
    task_kind: &'static str, field: impl Into<String>, message: impl Into<String>,
) -> TaskValidationError {
    TaskValidationError::InvalidField {
        task_kind,
        field: field.into(),
        message: message.into(),
    }
}

pub(crate) fn invalid_task(
    task_kind: &'static str, message: impl Into<String>,
) -> TaskValidationError {
    TaskValidationError::InvalidTask {
        task_kind,
        message: message.into(),
    }
}

pub(crate) fn resolve_required<T>(
    task_kind: &'static str, field: &'static str, value: Field<T>, context: &Table,
) -> Result<T, TaskValidationError>
where
    T: ResolveValue,
{
    resolve_optional(task_kind, field, value, context)?
        .ok_or_else(|| missing_field(task_kind, field))
}

pub(crate) fn resolve_optional<T>(
    task_kind: &'static str, field: &'static str, value: Field<T>, context: &Table,
) -> Result<Option<T>, TaskValidationError>
where
    T: ResolveValue,
{
    value
        .resolve(context)
        .map_err(|error| invalid_field(task_kind, field, error.to_string()))
}

pub(crate) fn resolve_or_default<T>(
    task_kind: &'static str, field: &'static str, value: Field<T>, context: &Table,
    default: impl FnOnce() -> T,
) -> Result<T, TaskValidationError>
where
    T: ResolveValue,
{
    Ok(resolve_optional(task_kind, field, value, context)?.unwrap_or_else(default))
}

impl<D> TaskResult<D> {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            status: TaskStatus::Ok,
            message: Some(message.into()),
            details: None,
        }
    }

    pub fn changed(message: impl Into<String>) -> Self {
        Self {
            status: TaskStatus::Changed,
            message: Some(message.into()),
            details: None,
        }
    }

    pub fn skipped(message: impl Into<String>) -> Self {
        Self {
            status: TaskStatus::Skipped,
            message: Some(message.into()),
            details: None,
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            status: TaskStatus::Failed,
            message: Some(message.into()),
            details: None,
        }
    }

    pub fn unreachable(message: impl Into<String>) -> Self {
        Self {
            status: TaskStatus::Unreachable,
            message: Some(message.into()),
            details: None,
        }
    }

    pub fn with_details(mut self, details: D) -> Self {
        self.details = Some(details);
        self
    }

    pub fn try_map_details<U, E>(
        self, f: impl FnOnce(D) -> Result<U, E>,
    ) -> Result<TaskResult<U>, E> {
        Ok(TaskResult {
            status: self.status,
            message: self.message,
            details: match self.details {
                Some(details) => Some(f(details)?),
                None => None,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use toml::Table;
    use url::Url;

    #[test]
    fn task_resolves_into_task_data() {
        let task = Task::File(file::FileTask {
            name: "ensure example file".into(),
            path: PathBuf::from("/tmp/example").into(),
            state: file::FileState::File.into(),
            owner: "root".into(),
            group: Field::Nil,
            mode: "0644".into(),
            content: "hello".into(),
        });

        let resolved = task.resolve(&Table::new()).unwrap();

        assert_eq!(
            resolved,
            TaskData::File(file::FileTaskData {
                name: Some("ensure example file".to_string()),
                path: PathBuf::from("/tmp/example"),
                state: file::FileState::File,
                owner: Some("root".to_string()),
                group: None,
                mode: Some("0644".to_string()),
                content: Some("hello".to_string()),
            })
        );
    }

    #[test]
    fn task_resolves_templates_before_transport() {
        let context = toml::toml! {
            region = "cn-north-1"
        };

        let resolved = Task::File(file::FileTask {
            name: "render example template".into(),
            path: PathBuf::from("/tmp/example").into(),
            state: file::FileState::File.into(),
            content: Field::tpl("hello {{ region }}"),
            owner: Field::Nil,
            group: Field::Nil,
            mode: Field::Nil,
        })
        .resolve(&context)
        .unwrap();

        assert_eq!(
            resolved,
            TaskData::File(file::FileTaskData {
                name: Some("render example template".to_string()),
                path: PathBuf::from("/tmp/example"),
                state: file::FileState::File,
                content: Some("hello cn-north-1".to_string()),
                owner: None,
                group: None,
                mode: None,
            })
        );
    }

    #[test]
    fn task_data_round_trips_as_json() {
        let task = TaskData::File(file::FileTaskData {
            name: Some("ensure example file".to_string()),
            path: PathBuf::from("/tmp/example"),
            state: file::FileState::File,
            owner: Some("root".to_string()),
            group: None,
            mode: Some("0644".to_string()),
            content: Some("hello".to_string()),
        });

        let json = serde_json::to_string(&task).unwrap();
        let decoded: TaskData = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, task);
    }

    #[test]
    fn task_request_round_trips_as_json() {
        let request = TaskRequest::new(file::FileTaskData {
            name: Some("render example template".to_string()),
            path: PathBuf::from("/tmp/example"),
            state: file::FileState::File,
            content: Some("hello cn-north-1".to_string()),
            owner: None,
            group: None,
            mode: None,
        });

        let json = serde_json::to_string(&request).unwrap();
        let decoded: TaskRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, request);
    }

    #[test]
    fn task_result_with_details_round_trips_as_json() {
        let result =
            TaskResult::changed("updated").with_details(TaskDetails::File(file::FileDetails {
                path: PathBuf::from("/tmp/example"),
                state: file::FileState::File,
                created: true,
                removed: false,
                content_changed: true,
                mode_changed: false,
                ownership_changed: false,
            }));

        let json = serde_json::to_string(&result).unwrap();
        let decoded: TaskResult = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, result);
    }

    #[test]
    fn task_result_with_command_details_round_trips_as_json() {
        let result = TaskResult::changed("command executed").with_details(TaskDetails::Command(
            command::CommandDetails {
                cmd: vec!["echo".to_string(), "hello".to_string()],
                chdir: Some(PathBuf::from("/tmp")),
                rc: Some(0),
                stdout: "hello\n".to_string(),
                stderr: String::new(),
            },
        ));

        let json = serde_json::to_string(&result).unwrap();
        let decoded: TaskResult = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, result);
    }

    #[test]
    fn task_result_with_download_details_round_trips_as_json() {
        let result = TaskResult::changed("downloaded file").with_details(TaskDetails::Download(
            download::DownloadDetails {
                url: Url::parse("https://example.com/archive.tar.gz").unwrap(),
                dest: PathBuf::from("/tmp/archive.tar.gz"),
                downloaded: true,
                bytes_written: 42,
                mode_changed: false,
                ownership_changed: false,
            },
        ));

        let json = serde_json::to_string(&result).unwrap();
        let decoded: TaskResult = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, result);
    }

    #[test]
    fn task_result_with_facts_details_round_trips_as_json() {
        let result = TaskResult::ok("collected host facts").with_details(TaskDetails::Facts(
            facts::FactsDetails {
                hostname: "node-1".to_string(),
            },
        ));

        let json = serde_json::to_string(&result).unwrap();
        let decoded: TaskResult = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, result);
    }

    #[test]
    fn task_result_with_stat_details_round_trips_as_json() {
        let result =
            TaskResult::ok("path inspected").with_details(TaskDetails::Stat(stat::StatDetails {
                path: PathBuf::from("/tmp/example"),
                exists: true,
                is_file: true,
                is_dir: false,
                is_symlink: false,
                mode: Some("0644".to_string()),
            }));

        let json = serde_json::to_string(&result).unwrap();
        let decoded: TaskResult = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, result);
    }

    #[test]
    fn task_result_with_copy_details_round_trips_as_json() {
        let result =
            TaskResult::changed("copied file").with_details(TaskDetails::Copy(copy::CopyDetails {
                src: PathBuf::from("/tmp/src"),
                dest: PathBuf::from("/tmp/dest"),
                created: true,
                content_changed: true,
                mode_changed: false,
                ownership_changed: false,
            }));

        let json = serde_json::to_string(&result).unwrap();
        let decoded: TaskResult = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, result);
    }
}
