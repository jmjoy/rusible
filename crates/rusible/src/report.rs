use crate::meta::{TaskDetails, TaskResult, TaskSpec, TaskStatus, TaskValidationError};
use rusible_template::TemplateError;
use std::{backtrace::Backtrace, path::PathBuf};

/// Result of a task run on the local controller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalRunReport<D = TaskDetails> {
    pub exec_path: PathBuf,
    pub result: TaskResult<D>,
}

impl From<TemplateError> for RuntimeError {
    fn from(source: TemplateError) -> Self {
        Self::TemplateRender {
            message: source.to_string(),
            backtrace: Backtrace::capture(),
        }
    }
}

impl From<TaskValidationError> for RuntimeError {
    fn from(source: TaskValidationError) -> Self {
        Self::TaskValidation {
            message: source.to_string(),
            backtrace: Backtrace::capture(),
        }
    }
}

/// Result of a task run on a remote host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteRunReport<D = TaskDetails> {
    pub host: String,
    pub exec_path: String,
    pub result: TaskResult<D>,
}

/// Aggregated results from multiple remote hosts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchRunReport<D = TaskDetails>(pub Vec<RemoteRunReport<D>>);

/// Infrastructure error returned while preparing or executing a run.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("{source}")]
    Io {
        #[from]
        source: std::io::Error,
        backtrace: Backtrace,
    },

    #[error("{source}")]
    Json {
        #[from]
        source: serde_json::Error,
        backtrace: Backtrace,
    },

    #[error("HOME is not set")]
    MissingHome { backtrace: Backtrace },

    #[error("child process `{command}` did not provide a {stream} handle")]
    MissingPipe {
        command: String,
        stream: &'static str,
        backtrace: Backtrace,
    },

    #[error("command failed with status {status}: {command}; stderr: {stderr}")]
    CommandFailed {
        command: String,
        status: i32,
        stderr: String,
        backtrace: Backtrace,
    },

    #[error(
        "exec availability check failed with status {status}: {command}; stdout: {stdout}; \
         stderr: {stderr}"
    )]
    ExecUnavailable {
        command: String,
        status: i32,
        stdout: String,
        stderr: String,
        backtrace: Backtrace,
    },

    #[error("{source}")]
    Russh {
        #[from]
        source: russh::Error,
        backtrace: Backtrace,
    },

    #[error("remote authentication failed: {message}")]
    AuthenticationFailed {
        message: String,
        backtrace: Backtrace,
    },

    #[error("remote connection requires either a password or a private key")]
    MissingCredentials { backtrace: Backtrace },

    #[error("remote SSH/SFTP protocol error: {message}")]
    RemoteProtocol {
        message: String,
        backtrace: Backtrace,
    },

    #[error("template render failed: {message}")]
    TemplateRender {
        message: String,
        backtrace: Backtrace,
    },

    #[error("task validation failed: {message}")]
    TaskValidation {
        message: String,
        backtrace: Backtrace,
    },

    #[error("{source}")]
    ShellQuote {
        #[from]
        source: shlex::QuoteError,
        backtrace: Backtrace,
    },

    #[error("tokio task join failed: {source}")]
    TaskJoin {
        #[from]
        source: tokio::task::JoinError,
        backtrace: Backtrace,
    },

    #[error("runnable has not been initialized with rusible-exec bytes")]
    NotInitialized { backtrace: Backtrace },

    #[error("task returned unexpected details kind `{actual}`; expected `{expected}`")]
    UnexpectedTaskDetails {
        expected: &'static str,
        actual: &'static str,
        backtrace: Backtrace,
    },
}

/// Error returned when a local task run does not succeed.
#[derive(Debug, thiserror::Error)]
pub enum LocalRunError<D = TaskDetails> {
    #[error("{0}")]
    Runtime(#[from] RuntimeError),

    #[error("local task returned a non-success status: {}", format_local_report(.0))]
    Report(LocalRunReport<D>),
}

/// Error returned when a single remote task run does not succeed.
#[derive(Debug, thiserror::Error)]
pub enum RemoteRunError<D = TaskDetails> {
    #[error("{0}")]
    Runtime(#[from] RuntimeError),

    #[error("remote task returned a non-success status: {}", format_remote_report(.0))]
    Report(RemoteRunReport<D>),
}

/// Error returned when a batch remote task run contains non-success results.
#[derive(Debug, thiserror::Error)]
pub enum BatchRunError<D = TaskDetails> {
    #[error("{0}")]
    Runtime(#[from] RuntimeError),

    #[error("batch task returned non-success statuses: {}", format_batch_report(.0))]
    Report(BatchRunReport<D>),
}

/// Helper methods for the results returned by `Runnable::run`.
pub trait RunResultExt: Sized {
    type Report;
    type Error;

    /// Returns true when at least one result has status `failed`.
    fn has_failed(&self) -> bool;

    /// Returns true when at least one result has status `unreachable`.
    fn has_unreachable(&self) -> bool;

    /// Returns true when at least one result has status `skipped`.
    fn has_skipped(&self) -> bool;

    /// Returns true when the result contains `unreachable` statuses and no
    /// `failed` statuses.
    fn only_unreachable(&self) -> bool;

    /// Extracts the report when one exists.
    fn into_report(self) -> Result<Self::Report, RuntimeError>;

    /// Converts a report-only unreachable error back into a success value.
    fn ignore_unreachable(self) -> Result<Self::Report, Self::Error>;

    /// Promotes any skipped result into an error.
    fn fail_on_skipped(self) -> Result<Self::Report, Self::Error>;
}

pub(crate) trait RunReportLike: Sized {
    fn has_status(&self, status: TaskStatus) -> bool;
}

pub(crate) trait ReportBackedError<R>: Sized {
    fn from_runtime_error(error: RuntimeError) -> Self;
    fn from_report(report: R) -> Self;
    fn report(&self) -> Option<&R>;
    fn into_report(self) -> Result<R, RuntimeError>;
}

impl<R, E> RunResultExt for Result<R, E>
where
    R: RunReportLike,
    E: ReportBackedError<R>,
{
    type Error = E;
    type Report = R;

    fn has_failed(&self) -> bool {
        report_ref(self).is_some_and(|report| report.has_status(TaskStatus::Failed))
    }

    fn has_unreachable(&self) -> bool {
        report_ref(self).is_some_and(|report| report.has_status(TaskStatus::Unreachable))
    }

    fn has_skipped(&self) -> bool {
        report_ref(self).is_some_and(|report| report.has_status(TaskStatus::Skipped))
    }

    fn only_unreachable(&self) -> bool {
        report_ref(self).is_some_and(report_only_unreachable)
    }

    fn into_report(self) -> Result<Self::Report, RuntimeError> {
        match self {
            Ok(report) => Ok(report),
            Err(error) => error.into_report(),
        }
    }

    fn ignore_unreachable(self) -> Result<Self::Report, Self::Error> {
        match self {
            Ok(report) => Ok(report),
            Err(error) => match error.into_report() {
                Ok(report) if report_only_unreachable(&report) => Ok(report),
                Ok(report) => Err(E::from_report(report)),
                Err(error) => Err(E::from_runtime_error(error)),
            },
        }
    }

    fn fail_on_skipped(self) -> Result<Self::Report, Self::Error> {
        match self {
            Ok(report) if report.has_status(TaskStatus::Skipped) => Err(E::from_report(report)),
            other => other,
        }
    }
}

fn report_ref<R, E>(result: &Result<R, E>) -> Option<&R>
where
    E: ReportBackedError<R>,
{
    match result {
        Ok(report) => Some(report),
        Err(error) => error.report(),
    }
}

impl LocalRunReport<TaskDetails> {
    pub(crate) fn try_into_typed<T>(self) -> Result<LocalRunReport<T::Details>, RuntimeError>
    where
        T: TaskSpec,
    {
        Ok(LocalRunReport {
            exec_path: self.exec_path,
            result: self
                .result
                .try_map_details(|details| typed_details::<T>(details))?,
        })
    }
}

impl RemoteRunReport<TaskDetails> {
    pub(crate) fn try_into_typed<T>(self) -> Result<RemoteRunReport<T::Details>, RuntimeError>
    where
        T: TaskSpec,
    {
        Ok(RemoteRunReport {
            host: self.host,
            exec_path: self.exec_path,
            result: self
                .result
                .try_map_details(|details| typed_details::<T>(details))?,
        })
    }
}

impl BatchRunReport<TaskDetails> {
    pub(crate) fn try_into_typed<T>(self) -> Result<BatchRunReport<T::Details>, RuntimeError>
    where
        T: TaskSpec,
    {
        Ok(BatchRunReport(
            self.0
                .into_iter()
                .map(RemoteRunReport::try_into_typed::<T>)
                .collect::<Result<Vec<_>, _>>()?,
        ))
    }
}

impl<D> RunReportLike for LocalRunReport<D> {
    fn has_status(&self, status: TaskStatus) -> bool {
        self.result.status == status
    }
}

impl<D> RunReportLike for RemoteRunReport<D> {
    fn has_status(&self, status: TaskStatus) -> bool {
        self.result.status == status
    }
}

impl<D> RunReportLike for BatchRunReport<D> {
    fn has_status(&self, status: TaskStatus) -> bool {
        self.0.iter().any(|report| report.result.status == status)
    }
}

impl<D> ReportBackedError<LocalRunReport<D>> for LocalRunError<D> {
    fn from_runtime_error(error: RuntimeError) -> Self {
        Self::Runtime(error)
    }

    fn from_report(report: LocalRunReport<D>) -> Self {
        Self::Report(report)
    }

    fn report(&self) -> Option<&LocalRunReport<D>> {
        match self {
            Self::Runtime(_) => None,
            Self::Report(report) => Some(report),
        }
    }

    fn into_report(self) -> Result<LocalRunReport<D>, RuntimeError> {
        match self {
            Self::Runtime(error) => Err(error),
            Self::Report(report) => Ok(report),
        }
    }
}

impl<D> ReportBackedError<RemoteRunReport<D>> for RemoteRunError<D> {
    fn from_runtime_error(error: RuntimeError) -> Self {
        Self::Runtime(error)
    }

    fn from_report(report: RemoteRunReport<D>) -> Self {
        Self::Report(report)
    }

    fn report(&self) -> Option<&RemoteRunReport<D>> {
        match self {
            Self::Runtime(_) => None,
            Self::Report(report) => Some(report),
        }
    }

    fn into_report(self) -> Result<RemoteRunReport<D>, RuntimeError> {
        match self {
            Self::Runtime(error) => Err(error),
            Self::Report(report) => Ok(report),
        }
    }
}

impl<D> ReportBackedError<BatchRunReport<D>> for BatchRunError<D> {
    fn from_runtime_error(error: RuntimeError) -> Self {
        Self::Runtime(error)
    }

    fn from_report(report: BatchRunReport<D>) -> Self {
        Self::Report(report)
    }

    fn report(&self) -> Option<&BatchRunReport<D>> {
        match self {
            Self::Runtime(_) => None,
            Self::Report(report) => Some(report),
        }
    }

    fn into_report(self) -> Result<BatchRunReport<D>, RuntimeError> {
        match self {
            Self::Runtime(error) => Err(error),
            Self::Report(report) => Ok(report),
        }
    }
}

pub(crate) fn classify_report<R, E>(report: R) -> Result<R, E>
where
    R: RunReportLike,
    E: ReportBackedError<R>,
{
    if report.has_status(TaskStatus::Failed) || report.has_status(TaskStatus::Unreachable) {
        Err(E::from_report(report))
    } else {
        Ok(report)
    }
}

fn report_only_unreachable<R>(report: &R) -> bool
where
    R: RunReportLike,
{
    report.has_status(TaskStatus::Unreachable) && !report.has_status(TaskStatus::Failed)
}

fn typed_details<T>(details: TaskDetails) -> Result<T::Details, RuntimeError>
where
    T: TaskSpec,
{
    let actual = details.kind();
    T::try_from_details(details).ok_or_else(|| RuntimeError::UnexpectedTaskDetails {
        expected: T::expected_task_kind(),
        actual,
        backtrace: Backtrace::capture(),
    })
}

fn format_local_report<D>(report: &LocalRunReport<D>) -> String {
    format_task_result(&report.result)
}

fn format_remote_report<D>(report: &RemoteRunReport<D>) -> String {
    format!("{} [{}]", report.host, format_task_result(&report.result))
}

fn format_batch_report<D>(report: &BatchRunReport<D>) -> String {
    let statuses = report
        .0
        .iter()
        .filter(|report| {
            matches!(
                report.result.status,
                TaskStatus::Failed | TaskStatus::Unreachable | TaskStatus::Skipped
            )
        })
        .map(format_remote_report)
        .collect::<Vec<_>>();

    if statuses.is_empty() {
        "no non-success statuses".to_string()
    } else {
        statuses.join(", ")
    }
}

fn format_task_result<D>(result: &TaskResult<D>) -> String {
    let status = result.status.as_str();

    match result.message.as_deref() {
        Some(message) if !message.is_empty() => format!("{status}: {message}"),
        _ => status.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_report(status: TaskStatus) -> LocalRunReport {
        LocalRunReport {
            exec_path: PathBuf::from("/tmp/rusible-exec"),
            result: TaskResult {
                status,
                message: Some(status_name(status).to_string()),
                details: None,
            },
        }
    }

    fn batch_report(statuses: &[TaskStatus]) -> BatchRunReport {
        BatchRunReport(
            statuses
                .iter()
                .enumerate()
                .map(|(index, status)| RemoteRunReport {
                    host: format!("host-{index}"),
                    exec_path: "/tmp/rusible-exec".to_string(),
                    result: TaskResult {
                        status: *status,
                        message: Some(status_name(*status).to_string()),
                        details: None,
                    },
                })
                .collect(),
        )
    }

    fn status_name(status: TaskStatus) -> &'static str {
        match status {
            TaskStatus::Ok => "ok",
            TaskStatus::Changed => "changed",
            TaskStatus::Skipped => "skipped",
            TaskStatus::Failed => "failed",
            TaskStatus::Unreachable => "unreachable",
        }
    }

    #[test]
    fn ignore_unreachable_recovers_report() {
        let result: Result<BatchRunReport, BatchRunError> =
            Err(BatchRunError::Report(batch_report(&[
                TaskStatus::Ok,
                TaskStatus::Unreachable,
            ])));

        let report = result.ignore_unreachable().unwrap();
        assert!(report.has_status(TaskStatus::Unreachable));
    }

    #[test]
    fn ignore_unreachable_does_not_hide_failed_statuses() {
        let result: Result<BatchRunReport, BatchRunError> =
            Err(BatchRunError::Report(batch_report(&[
                TaskStatus::Failed,
                TaskStatus::Unreachable,
            ])));

        assert!(matches!(
            result.ignore_unreachable(),
            Err(BatchRunError::Report(_))
        ));
    }

    #[test]
    fn fail_on_skipped_promotes_success_to_error() {
        let result: Result<BatchRunReport, BatchRunError> =
            Ok(batch_report(&[TaskStatus::Ok, TaskStatus::Skipped]));

        assert!(matches!(
            result.fail_on_skipped(),
            Err(BatchRunError::Report(_))
        ));
    }

    #[test]
    fn result_helpers_reflect_report_statuses() {
        let result: Result<LocalRunReport, LocalRunError> =
            Err(LocalRunError::Report(local_report(TaskStatus::Failed)));

        assert!(result.has_failed());
        assert!(!result.has_unreachable());
        assert!(!result.has_skipped());
        assert!(!result.only_unreachable());
    }

    #[test]
    fn into_report_preserves_runtime_errors() {
        let result: Result<LocalRunReport, LocalRunError> =
            Err(LocalRunError::Runtime(RuntimeError::NotInitialized {
                backtrace: Backtrace::capture(),
            }));

        assert!(matches!(
            result.into_report(),
            Err(RuntimeError::NotInitialized { .. })
        ));
    }
}
