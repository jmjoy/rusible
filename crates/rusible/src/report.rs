use crate::meta::{TaskResult, TaskStatus};
use std::{backtrace::Backtrace, path::PathBuf};

/// Result of a task run on the local controller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalRunReport {
    pub exec_path: PathBuf,
    pub result: TaskResult,
}

/// Result of a task run on a remote host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteRunReport {
    pub host: String,
    pub exec_path: String,
    pub result: TaskResult,
}

/// Aggregated results from multiple remote hosts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchRunReport {
    pub results: Vec<RemoteRunReport>,
}

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
        "exec availability check failed with status {status}: {command}; stdout: {stdout}; stderr: {stderr}"
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

    #[error("tokio task join failed: {source}")]
    TaskJoin {
        #[from]
        source: tokio::task::JoinError,
        backtrace: Backtrace,
    },

    #[error("runnable has not been initialized with rusible-exec bytes")]
    NotInitialized { backtrace: Backtrace },
}

/// Error returned when a local task run does not succeed.
#[derive(Debug, thiserror::Error)]
pub enum LocalRunError {
    #[error("{0}")]
    Runtime(#[from] RuntimeError),

    #[error("local task returned a non-success status: {}", format_local_report(.0))]
    Report(LocalRunReport),
}

/// Error returned when a single remote task run does not succeed.
#[derive(Debug, thiserror::Error)]
pub enum RemoteRunError {
    #[error("{0}")]
    Runtime(#[from] RuntimeError),

    #[error("remote task returned a non-success status: {}", format_remote_report(.0))]
    Report(RemoteRunReport),
}

/// Error returned when a batch remote task run contains non-success results.
#[derive(Debug, thiserror::Error)]
pub enum BatchRunError {
    #[error("{0}")]
    Runtime(#[from] RuntimeError),

    #[error("batch task returned non-success statuses: {}", format_batch_report(.0))]
    Report(BatchRunReport),
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
    type Report = R;
    type Error = E;

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

impl RunReportLike for LocalRunReport {
    fn has_status(&self, status: TaskStatus) -> bool {
        self.result.status == status
    }
}

impl RunReportLike for RemoteRunReport {
    fn has_status(&self, status: TaskStatus) -> bool {
        self.result.status == status
    }
}

impl RunReportLike for BatchRunReport {
    fn has_status(&self, status: TaskStatus) -> bool {
        self.results.iter().any(|report| report.result.status == status)
    }
}

impl ReportBackedError<LocalRunReport> for LocalRunError {
    fn from_runtime_error(error: RuntimeError) -> Self {
        Self::Runtime(error)
    }

    fn from_report(report: LocalRunReport) -> Self {
        Self::Report(report)
    }

    fn report(&self) -> Option<&LocalRunReport> {
        match self {
            Self::Runtime(_) => None,
            Self::Report(report) => Some(report),
        }
    }

    fn into_report(self) -> Result<LocalRunReport, RuntimeError> {
        match self {
            Self::Runtime(error) => Err(error),
            Self::Report(report) => Ok(report),
        }
    }
}

impl ReportBackedError<RemoteRunReport> for RemoteRunError {
    fn from_runtime_error(error: RuntimeError) -> Self {
        Self::Runtime(error)
    }

    fn from_report(report: RemoteRunReport) -> Self {
        Self::Report(report)
    }

    fn report(&self) -> Option<&RemoteRunReport> {
        match self {
            Self::Runtime(_) => None,
            Self::Report(report) => Some(report),
        }
    }

    fn into_report(self) -> Result<RemoteRunReport, RuntimeError> {
        match self {
            Self::Runtime(error) => Err(error),
            Self::Report(report) => Ok(report),
        }
    }
}

impl ReportBackedError<BatchRunReport> for BatchRunError {
    fn from_runtime_error(error: RuntimeError) -> Self {
        Self::Runtime(error)
    }

    fn from_report(report: BatchRunReport) -> Self {
        Self::Report(report)
    }

    fn report(&self) -> Option<&BatchRunReport> {
        match self {
            Self::Runtime(_) => None,
            Self::Report(report) => Some(report),
        }
    }

    fn into_report(self) -> Result<BatchRunReport, RuntimeError> {
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

fn format_local_report(report: &LocalRunReport) -> String {
    format_task_result(&report.result)
}

fn format_remote_report(report: &RemoteRunReport) -> String {
    format!("{} [{}]", report.host, format_task_result(&report.result))
}

fn format_batch_report(report: &BatchRunReport) -> String {
    let statuses = report
        .results
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

fn format_task_result(result: &TaskResult) -> String {
    let status = match result.status {
        TaskStatus::Ok => "ok",
        TaskStatus::Changed => "changed",
        TaskStatus::Skipped => "skipped",
        TaskStatus::Failed => "failed",
        TaskStatus::Unreachable => "unreachable",
    };

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
            },
        }
    }

    fn batch_report(statuses: &[TaskStatus]) -> BatchRunReport {
        BatchRunReport {
            results: statuses
                .iter()
                .enumerate()
                .map(|(index, status)| RemoteRunReport {
                    host: format!("host-{index}"),
                    exec_path: "/tmp/rusible-exec".to_string(),
                    result: TaskResult {
                        status: *status,
                        message: Some(status_name(*status).to_string()),
                    },
                })
                .collect(),
        }
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
            Err(BatchRunError::Report(batch_report(&[TaskStatus::Ok, TaskStatus::Unreachable])));

        let report = result.ignore_unreachable().unwrap();
        assert!(report.has_status(TaskStatus::Unreachable));
    }

    #[test]
    fn ignore_unreachable_does_not_hide_failed_statuses() {
        let result: Result<BatchRunReport, BatchRunError> = Err(BatchRunError::Report(
            batch_report(&[TaskStatus::Failed, TaskStatus::Unreachable]),
        ));

        assert!(matches!(result.ignore_unreachable(), Err(BatchRunError::Report(_))));
    }

    #[test]
    fn fail_on_skipped_promotes_success_to_error() {
        let result: Result<BatchRunReport, BatchRunError> =
            Ok(batch_report(&[TaskStatus::Ok, TaskStatus::Skipped]));

        assert!(matches!(result.fail_on_skipped(), Err(BatchRunError::Report(_))));
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
        let result: Result<LocalRunReport, LocalRunError> = Err(LocalRunError::Runtime(
            RuntimeError::NotInitialized {
                backtrace: Backtrace::capture(),
            },
        ));

        assert!(matches!(
            result.into_report(),
            Err(RuntimeError::NotInitialized { .. })
        ));
    }
}
