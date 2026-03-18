use crate::{
    exec::{
        ensure_local_exec, execute_remote_task, initialize_remote_exec, run_exec_process,
        run_named_remote_with_json, run_remote_with_json, validate_local_exec,
        validate_remote_exec,
    },
    inventory::Inventory,
    meta::{Task, TaskResult},
    report::{
        classify_report, BatchRunError, BatchRunReport, LocalRunError, LocalRunReport,
        RemoteRunError, RemoteRunReport, RuntimeError,
    },
    runtime::Runnable,
};
use std::{
    backtrace::Backtrace,
    path::{Path, PathBuf},
};
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

/// Represents the local controller node.
#[derive(Debug, Default)]
pub struct Local {
    exec_path: Option<PathBuf>,
}

/// Represents an SSH-accessible managed host.
#[derive(Debug, Clone)]
pub struct Remote {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Option<String>,
    pub key: Option<PathBuf>,
    pub(crate) remote_exec_path: Option<String>,
}

impl Local {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn exec_path(&self) -> Option<&Path> {
        self.exec_path.as_deref()
    }
}

impl Remote {
    pub fn new(
        host: impl Into<String>,
        port: u16,
        user: impl Into<String>,
        password: Option<String>,
        key: Option<PathBuf>,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            user: user.into(),
            password,
            key,
            remote_exec_path: None,
        }
    }

    pub fn remote_exec_path(&self) -> Option<&str> {
        self.remote_exec_path.as_deref()
    }
}

impl Runnable for Local {
    type InitError = RuntimeError;
    type RunError = LocalRunError;
    type Output = LocalRunReport;

    async fn init(&mut self, exec_bytes: &[u8]) -> Result<(), Self::InitError> {
        let exec_path = ensure_local_exec(exec_bytes).await?;
        validate_local_exec(&exec_path).await?;
        info!(exec_path = %exec_path.display(), "initialized local rusible-exec");
        self.exec_path = Some(exec_path);
        Ok(())
    }

    async fn run<T>(&mut self, task: T) -> Result<Self::Output, Self::RunError>
    where
        T: Into<Task> + Send,
    {
        let task = task.into();
        let exec_path = self.exec_path.clone().ok_or_else(|| RuntimeError::NotInitialized {
            backtrace: Backtrace::capture(),
        })?;
        debug!(exec_path = %exec_path.display(), task = ?task, "running task locally");
        let result = run_exec_process(&exec_path, &task).await?;
        info!(exec_path = %exec_path.display(), status = ?result.status, "local task finished");

        classify_report(LocalRunReport { exec_path, result })
    }
}

impl Runnable for Remote {
    type InitError = RuntimeError;
    type RunError = RemoteRunError;
    type Output = RemoteRunReport;

    async fn init(&mut self, exec_bytes: &[u8]) -> Result<(), Self::InitError> {
        let remote_exec_path = initialize_remote_exec(self, exec_bytes).await?;
        validate_remote_exec(self, &remote_exec_path).await?;
        info!(host = %self.host, port = self.port, exec_path = %remote_exec_path, "initialized remote rusible-exec");
        self.remote_exec_path = Some(remote_exec_path);
        Ok(())
    }

    async fn run<T>(&mut self, task: T) -> Result<Self::Output, Self::RunError>
    where
        T: Into<Task> + Send,
    {
        let task = task.into();
        let exec_path = self
            .remote_exec_path
            .clone()
            .ok_or_else(|| RuntimeError::NotInitialized {
                backtrace: Backtrace::capture(),
            })?;
        debug!(host = %self.host, port = self.port, exec_path = %exec_path, task = ?task, "running task on remote host");
        let task_json = serde_json::to_string(&task).map_err(RuntimeError::from)?;
        let result = match execute_remote_task(self, &exec_path, &task_json).await {
            Ok(result) => result,
            Err(error) => {
                warn!(host = %self.host, port = self.port, exec_path = %exec_path, error = %error, "remote task became unreachable");
                TaskResult::unreachable(error.to_string())
            }
        };
        info!(host = %self.host, port = self.port, exec_path = %exec_path, status = ?result.status, "remote task finished");

        classify_report(RemoteRunReport {
            host: self.host.clone(),
            exec_path,
            result,
        })
    }
}

impl<I> Runnable for I
where
    I: Clone + IntoIterator<Item = Remote> + FromIterator<Remote> + Send,
{
    type InitError = RuntimeError;
    type RunError = BatchRunError;
    type Output = BatchRunReport;

    async fn init(&mut self, exec_bytes: &[u8]) -> Result<(), Self::InitError> {
        let remotes = self.clone().into_iter().collect::<Vec<_>>();
        info!(remote_count = remotes.len(), "initializing remote executables for batch");

        let mut initialized_remotes = Vec::new();

        for mut remote in remotes {
            remote.init(exec_bytes).await?;
            initialized_remotes.push(remote);
        }

        *self = initialized_remotes.into_iter().collect();
        Ok(())
    }

    async fn run<T>(&mut self, task: T) -> Result<Self::Output, Self::RunError>
    where
        T: Into<Task> + Send,
    {
        let task = task.into();
        let task_json = serde_json::to_string(&task).map_err(RuntimeError::from)?;
        let remotes = self.clone().into_iter().collect::<Vec<_>>();
        info!(remote_count = remotes.len(), task = ?task, "running batch task");
        let mut tasks = JoinSet::new();

        for (index, remote) in remotes.iter().cloned().enumerate() {
            let task_json = task_json.clone();
            tasks.spawn(async move { (index, run_remote_with_json(remote, task_json).await) });
        }

        let mut results = vec![None; remotes.len()];
        while let Some(joined) = tasks.join_next().await {
            let (index, report) = joined.map_err(RuntimeError::from)?;
            results[index] = Some(report?);
        }

        let results = results
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .expect("all spawned remote runs should produce a result");

        info!(remote_count = results.len(), "batch task finished");

        classify_report(BatchRunReport { results })
    }
}

impl Runnable for Inventory {
    type InitError = RuntimeError;
    type RunError = BatchRunError;
    type Output = BatchRunReport;

    async fn init(&mut self, exec_bytes: &[u8]) -> Result<(), Self::InitError> {
        let host_count = self.len();
        info!(host_count, "initializing remote executables for inventory");

        for host in &mut self.hosts {
            host.remote.init(exec_bytes).await?;
        }

        Ok(())
    }

    async fn run<T>(&mut self, task: T) -> Result<Self::Output, Self::RunError>
    where
        T: Into<Task> + Send,
    {
        let task = task.into();
        let task_json = serde_json::to_string(&task).map_err(RuntimeError::from)?;
        let hosts = self.collect_named_remotes();
        info!(host_count = hosts.len(), task = ?task, "running inventory task");

        let mut join_set = JoinSet::new();
        for (index, (name, remote)) in hosts.iter().cloned().enumerate() {
            let task_json = task_json.clone();
            join_set.spawn(async move {
                (index, run_named_remote_with_json(name, remote, task_json).await)
            });
        }

        let mut results = vec![None; hosts.len()];
        while let Some(joined) = join_set.join_next().await {
            let (index, report) = joined.map_err(RuntimeError::from)?;
            results[index] = Some(report?);
        }

        let results = results
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .expect("all spawned host runs should produce a result");

        info!(host_count = results.len(), "inventory task finished");

        classify_report(BatchRunReport { results })
    }
}
