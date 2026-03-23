use crate::{
    VarError,
    exec::{
        ensure_local_exec, execute_remote_task, initialize_remote_exec, run_exec_process,
        run_named_remote_with_json, run_remote_with_json, upload_remote_bytes,
        validate_local_exec,
        validate_remote_exec,
    },
    inventory::Inventory,
    meta::{Task, TaskRequest, TaskResult, TaskSpec},
    report::{
        BatchRunError, BatchRunReport, LocalRunError, LocalRunReport, RemoteRunError,
        RemoteRunReport, RuntimeError, classify_report,
    },
    runtime::Runnable,
    vars::{
        build_local_context, build_remote_context, merge_tables, remove_table_path, set_table_path,
    },
};
use std::{
    backtrace::Backtrace,
    path::{Path, PathBuf},
};
use tokio::task::JoinSet;
use toml::{Table, Value};
use tracing::{debug, info, warn};

/// Represents the local controller node.
#[derive(Debug, Default)]
pub struct Local {
    exec_path: Option<PathBuf>,
    vars: Table,
}

/// Represents an SSH-accessible managed host.
#[derive(Debug, Clone)]
pub struct Remote {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Option<String>,
    pub key: Option<PathBuf>,
    pub vars: Table,
    pub(crate) remote_exec_path: Option<String>,
}

/// Result of uploading controller-side bytes to a remote host path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadReport {
    pub remote_path: String,
    pub bytes_written: usize,
}

impl Local {
    /// Creates a local controller target with empty template variables.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a local controller target with the provided template
    /// variables.
    pub fn new_with_vars(vars: Table) -> Self {
        Self {
            exec_path: None,
            vars,
        }
    }

    /// Returns the prepared `rusible-exec` path when the target has been
    /// initialized.
    pub fn exec_path(&self) -> Option<&Path> {
        self.exec_path.as_deref()
    }

    /// Returns the template variable table for this local target.
    pub fn vars(&self) -> &Table {
        &self.vars
    }

    /// Returns the mutable template variable table for this local target.
    pub fn vars_mut(&mut self) -> &mut Table {
        &mut self.vars
    }

    /// Recursively merges variables into the local target.
    pub fn merge_vars(&mut self, vars: Table) {
        merge_tables(&mut self.vars, &vars);
    }

    /// Sets a variable by dotted path, creating missing intermediate tables.
    pub fn set_var(
        &mut self, path: impl AsRef<str>, value: impl Into<Value>,
    ) -> Result<(), VarError> {
        set_table_path(&mut self.vars, path.as_ref(), value)
    }

    /// Removes a variable by dotted path and returns the removed value when
    /// present.
    pub fn remove_var(&mut self, path: impl AsRef<str>) -> Result<Option<Value>, VarError> {
        remove_table_path(&mut self.vars, path.as_ref())
    }
}

impl Remote {
    /// Creates a remote target with empty template variables.
    pub fn new(
        host: impl Into<String>, port: u16, user: impl Into<String>, password: Option<String>,
        key: Option<PathBuf>,
    ) -> Self {
        Self::new_with_vars(host, port, user, password, key, Table::new())
    }

    /// Creates a remote target with the provided template variables.
    pub fn new_with_vars(
        host: impl Into<String>, port: u16, user: impl Into<String>, password: Option<String>,
        key: Option<PathBuf>, vars: Table,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            user: user.into(),
            password,
            key,
            vars,
            remote_exec_path: None,
        }
    }

    /// Returns the remote `rusible-exec` path when the target has been
    /// initialized.
    pub fn remote_exec_path(&self) -> Option<&str> {
        self.remote_exec_path.as_deref()
    }

    /// Recursively merges variables into the remote target.
    pub fn merge_vars(&mut self, vars: Table) {
        merge_tables(&mut self.vars, &vars);
    }

    /// Sets a variable by dotted path, creating missing intermediate tables.
    pub fn set_var(
        &mut self, path: impl AsRef<str>, value: impl Into<Value>,
    ) -> Result<(), VarError> {
        set_table_path(&mut self.vars, path.as_ref(), value)
    }

    /// Removes a variable by dotted path and returns the removed value when
    /// present.
    pub fn remove_var(&mut self, path: impl AsRef<str>) -> Result<Option<Value>, VarError> {
        remove_table_path(&mut self.vars, path.as_ref())
    }

    /// Uploads a local file from the controller to a path on the remote host.
    pub async fn upload_file<P, Q>(
        &self, local_path: P, remote_path: Q,
    ) -> Result<UploadReport, RuntimeError>
    where
        P: AsRef<Path>,
        Q: AsRef<Path>,
    {
        let bytes = tokio::fs::read(local_path.as_ref()).await?;
        self.upload_bytes(remote_path, &bytes).await
    }

    /// Uploads controller-provided bytes to a path on the remote host.
    pub async fn upload_bytes<P>(
        &self, remote_path: P, bytes: &[u8],
    ) -> Result<UploadReport, RuntimeError>
    where
        P: AsRef<Path>,
    {
        let remote_path = upload_remote_bytes(self, remote_path.as_ref(), bytes).await?;

        Ok(UploadReport {
            remote_path,
            bytes_written: bytes.len(),
        })
    }
}

impl Runnable for Local {
    type InitError = RuntimeError;
    type Output<D> = LocalRunReport<D>;
    type RunError<D> = LocalRunError<D>;

    async fn init(&mut self, exec_bytes: &[u8]) -> Result<(), Self::InitError> {
        let exec_path = ensure_local_exec(exec_bytes).await?;
        validate_local_exec(&exec_path).await?;
        info!(exec_path = %exec_path.display(), "initialized local rusible-exec");
        self.exec_path = Some(exec_path);
        Ok(())
    }

    async fn run<T>(
        &mut self, task: T,
    ) -> Result<Self::Output<T::Details>, Self::RunError<T::Details>>
    where
        T: TaskSpec + Send,
    {
        let task = task.into();
        let request = prepare_local_request(self, task.clone());
        let exec_path = self
            .exec_path
            .clone()
            .ok_or_else(|| RuntimeError::NotInitialized {
                backtrace: Backtrace::capture(),
            })?;
        debug!(exec_path = %exec_path.display(), task = ?task, "running task locally");
        let result = run_exec_process(&exec_path, &request).await?;
        info!(exec_path = %exec_path.display(), status = ?result.status, "local task finished");

        let report = LocalRunReport { exec_path, result }.try_into_typed::<T>()?;
        classify_report(report)
    }
}

impl Runnable for Remote {
    type InitError = RuntimeError;
    type Output<D> = RemoteRunReport<D>;
    type RunError<D> = RemoteRunError<D>;

    async fn init(&mut self, exec_bytes: &[u8]) -> Result<(), Self::InitError> {
        let remote_exec_path = initialize_remote_exec(self, exec_bytes).await?;
        validate_remote_exec(self, &remote_exec_path).await?;
        info!(host = %self.host, port = self.port, exec_path = %remote_exec_path, "initialized remote rusible-exec");
        self.remote_exec_path = Some(remote_exec_path);
        Ok(())
    }

    async fn run<T>(
        &mut self, task: T,
    ) -> Result<Self::Output<T::Details>, Self::RunError<T::Details>>
    where
        T: TaskSpec + Send,
    {
        let task = task.into();
        let request = prepare_remote_request(task.clone(), self, None, None);
        let exec_path =
            self.remote_exec_path
                .clone()
                .ok_or_else(|| RuntimeError::NotInitialized {
                    backtrace: Backtrace::capture(),
                })?;
        debug!(host = %self.host, port = self.port, exec_path = %exec_path, task = ?task, "running task on remote host");
        let task_json = serde_json::to_string(&request).map_err(RuntimeError::from)?;
        let result = match execute_remote_task(self, &exec_path, &task_json).await {
            Ok(result) => result,
            Err(error) => {
                warn!(host = %self.host, port = self.port, exec_path = %exec_path, error = %error, "remote task became unreachable");
                TaskResult::unreachable(error.to_string())
            }
        };
        info!(host = %self.host, port = self.port, exec_path = %exec_path, status = ?result.status, "remote task finished");

        let report = RemoteRunReport {
            host: self.host.clone(),
            exec_path,
            result,
        }
        .try_into_typed::<T>()?;

        classify_report(report)
    }
}

impl<I> Runnable for I
where
    I: Clone + IntoIterator<Item = Remote> + FromIterator<Remote> + Send,
{
    type InitError = RuntimeError;
    type Output<D> = BatchRunReport<D>;
    type RunError<D> = BatchRunError<D>;

    async fn init(&mut self, exec_bytes: &[u8]) -> Result<(), Self::InitError> {
        let remotes = self.clone().into_iter().collect::<Vec<_>>();
        info!(
            remote_count = remotes.len(),
            "initializing remote executables for batch"
        );

        let mut initialized_remotes = Vec::new();

        for mut remote in remotes {
            remote.init(exec_bytes).await?;
            initialized_remotes.push(remote);
        }

        *self = initialized_remotes.into_iter().collect();
        Ok(())
    }

    async fn run<T>(
        &mut self, task: T,
    ) -> Result<Self::Output<T::Details>, Self::RunError<T::Details>>
    where
        T: TaskSpec + Send,
    {
        let task = task.into();
        let remotes = self.clone().into_iter().collect::<Vec<_>>();
        info!(remote_count = remotes.len(), task = ?task, "running batch task");
        let mut tasks = JoinSet::new();

        for (index, remote) in remotes.iter().cloned().enumerate() {
            let request = prepare_remote_request(task.clone(), &remote, None, None);
            let task_json = serde_json::to_string(&request).map_err(RuntimeError::from)?;
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

        let report = BatchRunReport(results).try_into_typed::<T>()?;
        classify_report(report)
    }
}

impl Runnable for Inventory {
    type InitError = RuntimeError;
    type Output<D> = BatchRunReport<D>;
    type RunError<D> = BatchRunError<D>;

    async fn init(&mut self, exec_bytes: &[u8]) -> Result<(), Self::InitError> {
        let host_count = self.len();
        info!(host_count, "initializing remote executables for inventory");

        for host in &mut self.hosts {
            host.remote.init(exec_bytes).await?;
        }

        Ok(())
    }

    async fn run<T>(
        &mut self, task: T,
    ) -> Result<Self::Output<T::Details>, Self::RunError<T::Details>>
    where
        T: TaskSpec + Send,
    {
        let task = task.into();
        let hosts = self.hosts().to_vec();
        info!(host_count = hosts.len(), task = ?task, "running inventory task");

        let mut join_set = JoinSet::new();
        for (index, host) in hosts.iter().cloned().enumerate() {
            let request = prepare_remote_request(
                task.clone(),
                host.remote(),
                Some(self.vars()),
                Some(host.name()),
            );
            let task_json = serde_json::to_string(&request).map_err(RuntimeError::from)?;
            let name = host.name().to_string();
            let remote = host.remote().clone();
            join_set.spawn(async move {
                (
                    index,
                    run_named_remote_with_json(name, remote, task_json).await,
                )
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

        let report = BatchRunReport(results).try_into_typed::<T>()?;
        classify_report(report)
    }
}

fn prepare_local_request(local: &Local, task: Task) -> TaskRequest {
    TaskRequest::new(task, build_local_context(local.vars()))
}

fn prepare_remote_request(
    task: Task, remote: &Remote, inventory_vars: Option<&Table>, host_name: Option<&str>,
) -> TaskRequest {
    TaskRequest::new(
        task,
        build_remote_context(
            inventory_vars,
            &remote.vars,
            host_name,
            &remote.host,
            remote.port,
            &remote.user,
        ),
    )
}
