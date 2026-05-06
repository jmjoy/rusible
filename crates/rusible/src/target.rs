use crate::{
    exec::{
        ensure_local_exec, execute_remote_task, initialize_remote_exec, run_exec_process,
        run_named_remote_with_json, run_remote_with_json, upload_remote_bytes, validate_local_exec,
        validate_remote_exec,
    },
    inventory::Inventory,
    meta::{
        field::Field,
        task::{
            TaskData, TaskDataSpec, TaskDetails, TaskRequest, TaskResult, TaskSpec, TaskStatus,
        },
    },
    report::{
        BatchRunError, BatchRunReport, LocalRunError, LocalRunReport, RemoteRunError,
        RemoteRunReport, RuntimeError, classify_report,
    },
    runtime::Runnable,
    vars::{
        VarError, VarLookupError, build_local_context, build_remote_context, get_table_path_string,
        merge_tables, remove_table_path, set_table_path,
    },
};
use std::{
    backtrace::Backtrace,
    path::{Path, PathBuf},
};
use tokio::task::JoinSet;
use toml::{Table, Value};
use tracing::{Instrument, debug, info, info_span, warn};

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

/// Upload options applied after a file is written to the remote host.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UploadOptions {
    pub owner: Option<String>,
    pub group: Option<String>,
    pub mode: Option<String>,
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

    /// Returns a string variable by dotted path.
    pub fn get_var(&self, path: impl AsRef<str>) -> Result<String, VarLookupError> {
        get_table_path_string(&self.vars, path.as_ref())
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

    /// Returns the template variable table for this remote target.
    pub fn vars(&self) -> &Table {
        &self.vars
    }

    pub(crate) fn build_context(&self, defaults: Option<&Table>, host_name: Option<&str>) -> Table {
        build_remote_context(
            defaults, &self.vars, host_name, &self.host, self.port, &self.user,
        )
    }

    /// Returns a string variable by dotted path.
    pub fn get_var(&self, path: impl AsRef<str>) -> Result<String, VarLookupError> {
        get_table_path_string(&self.vars, path.as_ref())
    }

    /// Returns a string variable by dotted path when present.
    pub fn get_var_optional(
        &self, path: impl AsRef<str>,
    ) -> Result<Option<String>, VarLookupError> {
        match self.get_var(path) {
            Ok(value) => Ok(Some(value)),
            Err(VarLookupError::Missing { .. }) => Ok(None),
            Err(error) => Err(error),
        }
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
    pub async fn upload_file(
        &self, local_path: impl Into<Field<PathBuf>>, remote_path: impl Into<Field<PathBuf>>,
        options: UploadOptions,
    ) -> Result<UploadReport, RuntimeError> {
        let context = self.build_context(None, None);
        let local_path = resolve_upload_path(local_path.into(), "local_path", &context)?;
        let remote_path = resolve_upload_path(remote_path.into(), "remote_path", &context)?;
        let upload_span = info_span!(
            "UPLOAD",
            host = %self.host,
            port = self.port,
            local_path = %local_path.display(),
            remote_path = %remote_path.display()
        );
        let _upload_guard = upload_span.enter();
        let bytes = tokio::fs::read(&local_path).await?;
        self.upload_bytes(remote_path, &bytes, options).await
    }

    /// Uploads controller-provided bytes to a path on the remote host.
    pub async fn upload_bytes(
        &self, remote_path: impl Into<Field<PathBuf>>, bytes: &[u8], options: UploadOptions,
    ) -> Result<UploadReport, RuntimeError> {
        let context = self.build_context(None, None);
        let remote_path = resolve_upload_path(remote_path.into(), "remote_path", &context)?;
        let upload_span = info_span!(
            "UPLOAD",
            host = %self.host,
            port = self.port,
            remote_path = %remote_path.display(),
            bytes = bytes.len()
        );
        let _upload_guard = upload_span.enter();
        let remote_path = upload_remote_bytes(self, &remote_path, bytes, &options).await?;

        info!(bytes_written = bytes.len(), "uploaded file");

        Ok(UploadReport {
            remote_path,
            bytes_written: bytes.len(),
        })
    }
}

pub(crate) fn resolve_upload_path(
    path: Field<PathBuf>, field: &'static str, context: &Table,
) -> Result<PathBuf, RuntimeError> {
    path.resolve(context)
        .map_err(RuntimeError::from)?
        .ok_or_else(|| RuntimeError::TaskValidation {
            message: format!("upload field `{field}` is required"),
            backtrace: Backtrace::capture(),
        })
}

impl Runnable for Local {
    type InitError = RuntimeError;
    type Output<D> = LocalRunReport<D>;
    type RunError<D> = LocalRunError<D>;

    async fn init(&mut self, exec_bytes: &[u8]) -> Result<(), Self::InitError> {
        let init_span = info_span!("INIT", host = "localhost");
        let _init_guard = init_span.enter();
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
        T: TaskSpec + Clone + Send,
    {
        let request = prepare_local_request(self, task)?;
        let task_name = request.task.display_name().to_string();
        let task_kind = request.task.kind();
        let task_span =
            info_span!("TASK", name = %task_name, kind = task_kind, target = "localhost");
        let _task_guard = task_span.enter();
        let host_span = info_span!(parent: &task_span, "HOST", host = "localhost");
        let _host_guard = host_span.enter();
        let exec_path = self
            .exec_path
            .clone()
            .ok_or_else(|| RuntimeError::NotInitialized {
                backtrace: Backtrace::capture(),
            })?;
        debug!(task = ?request.task, "running task locally");
        let result = run_exec_process(&exec_path, &request).await?;
        emit_task_result_events(&result);

        let report = LocalRunReport { exec_path, result }.try_into_typed::<T>()?;
        classify_report(report)
    }
}

impl Runnable for Remote {
    type InitError = RuntimeError;
    type Output<D> = RemoteRunReport<D>;
    type RunError<D> = RemoteRunError<D>;

    async fn init(&mut self, exec_bytes: &[u8]) -> Result<(), Self::InitError> {
        let init_span = info_span!("INIT", host = %self.host, port = self.port);
        let _init_guard = init_span.enter();
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
        T: TaskSpec + Clone + Send,
    {
        let request = prepare_remote_request(task, self, None, None)?;
        let task_name = request.task.display_name().to_string();
        let task_kind = request.task.kind();
        let task_span =
            info_span!("TASK", name = %task_name, kind = task_kind, target = %self.host);
        let _task_guard = task_span.enter();
        let exec_path =
            self.remote_exec_path
                .clone()
                .ok_or_else(|| RuntimeError::NotInitialized {
                    backtrace: Backtrace::capture(),
                })?;
        let host_span = info_span!(parent: &task_span, "HOST", host = %self.host, port = self.port);
        let _host_guard = host_span.enter();
        debug!(host = %self.host, port = self.port, task = ?request.task, "running task on remote host");
        let task_json = serde_json::to_string(&request).map_err(RuntimeError::from)?;
        let result = match execute_remote_task(self, &exec_path, &task_json).await {
            Ok(result) => result,
            Err(error) => {
                warn!(host = %self.host, port = self.port, error = %error, "remote task became unreachable");
                TaskResult::unreachable(error.to_string())
            }
        };
        emit_task_result_events(&result);

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
        let init_span = info_span!("INIT", scope = "batch", remote_count = remotes.len());
        let _init_guard = init_span.enter();

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
        T: TaskSpec + Clone + Send,
    {
        let remotes = self.clone().into_iter().collect::<Vec<_>>();
        let task_kind = T::expected_task_kind();
        let task_span = info_span!(
            "TASK",
            name = task_kind,
            kind = task_kind,
            remote_count = remotes.len()
        );
        let _task_guard = task_span.enter();
        debug!(
            remote_count = remotes.len(),
            kind = task_kind,
            "running batch task"
        );
        let mut tasks = JoinSet::new();

        for (index, remote) in remotes.iter().cloned().enumerate() {
            let request = prepare_remote_request(task.clone(), &remote, None, None)?;
            let task_json = serde_json::to_string(&request).map_err(RuntimeError::from)?;
            let host_name = remote.host.clone();
            let host_port = remote.port;
            let host_span =
                info_span!(parent: &task_span, "HOST", host = %host_name, port = host_port);
            tasks.spawn(
                async move {
                    let report = run_remote_with_json(remote, task_json).await;

                    match &report {
                        Ok(report) => emit_task_result_events(&report.result),
                        Err(error) => {
                            warn!(error = %error, "host task failed before report classification")
                        }
                    }

                    (index, report)
                }
                .instrument(host_span),
            );
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
        let init_span = info_span!("INIT", scope = "inventory", host_count);
        let _init_guard = init_span.enter();

        for host in &mut self.hosts {
            host.remote.init(exec_bytes).await?;
        }

        Ok(())
    }

    async fn run<T>(
        &mut self, task: T,
    ) -> Result<Self::Output<T::Details>, Self::RunError<T::Details>>
    where
        T: TaskSpec + Clone + Send,
    {
        let hosts = self.hosts().to_vec();
        let task_kind = T::expected_task_kind();
        let task_span = info_span!(
            "TASK",
            name = task_kind,
            kind = task_kind,
            host_count = hosts.len()
        );
        let _task_guard = task_span.enter();
        debug!(
            host_count = hosts.len(),
            kind = task_kind,
            "running inventory task"
        );

        let mut join_set = JoinSet::new();
        for (index, host) in hosts.iter().cloned().enumerate() {
            let request = prepare_remote_request(
                task.clone(),
                host.remote(),
                Some(self.vars()),
                Some(host.name()),
            )?;
            let task_json = serde_json::to_string(&request).map_err(RuntimeError::from)?;
            let name = host.name().to_string();
            let remote = host.remote().clone();
            let host_port = remote.port;
            let host_span = info_span!(parent: &task_span, "HOST", host = %name, port = host_port);
            join_set.spawn(
                async move {
                    let report = run_named_remote_with_json(name, remote, task_json).await;

                    match &report {
                        Ok(report) => emit_task_result_events(&report.result),
                        Err(error) => {
                            warn!(error = %error, "host task failed before report classification")
                        }
                    }

                    (index, report)
                }
                .instrument(host_span),
            );
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

        let report = BatchRunReport(results).try_into_typed::<T>()?;
        classify_report(report)
    }
}

fn emit_task_result_events(result: &TaskResult) {
    let message = result
        .message
        .as_deref()
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| result.status.as_str());

    match result.status {
        TaskStatus::Failed | TaskStatus::Unreachable => {
            warn!(status = %result.status, "{message}");
            emit_failure_detail_events(result.details.as_ref());
        }
        _ => info!(status = %result.status, "{message}"),
    }
}

fn emit_failure_detail_events(details: Option<&TaskDetails>) {
    let Some(details) = details else {
        return;
    };

    match details {
        TaskDetails::Command(details) => {
            emit_command_output_events(details.rc, &details.stdout, &details.stderr)
        }
        TaskDetails::Shell(details) => {
            emit_command_output_events(details.rc, &details.stdout, &details.stderr)
        }
        _ => {}
    }
}

fn emit_command_output_events(rc: Option<i32>, stdout: &str, stderr: &str) {
    if let Some(rc) = rc {
        warn!(rc, "command exited with non-zero status");
    }

    if !stdout.trim().is_empty() {
        warn!(stream = "stdout", "{}", stdout.trim());
    }

    if !stderr.trim().is_empty() {
        warn!(stream = "stderr", "{}", stderr.trim());
    }
}

fn prepare_local_request<T>(local: &Local, task: T) -> Result<TaskRequest, RuntimeError>
where
    T: TaskSpec,
{
    let context = build_local_context(local.vars());
    let task = resolve_task(task, &context)?;
    Ok(TaskRequest::new(task))
}

fn prepare_remote_request<T>(
    task: T, remote: &Remote, inventory_vars: Option<&Table>, host_name: Option<&str>,
) -> Result<TaskRequest, RuntimeError>
where
    T: TaskSpec,
{
    let context = remote.build_context(inventory_vars, host_name);
    let task = resolve_task(task, &context)?;
    Ok(TaskRequest::new(task))
}

fn resolve_task<T>(task: T, context: &Table) -> Result<TaskData, RuntimeError>
where
    T: TaskSpec,
{
    let task = task.resolve(context).map_err(RuntimeError::from)?;
    task.validate().map_err(RuntimeError::from)?;
    Ok(task.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::field::{ResolveValueError, TemplateError};

    #[test]
    fn template_path_renders_remote_context_values() {
        let mut remote = Remote::new("10.0.0.11", 22, "root", None, None);
        remote.set_var("etcd.cert_dir", "/tmp/certs").unwrap();

        let context = remote.build_context(None, Some("web-1"));
        let rendered = resolve_upload_path(
            Field::tpl("{{ etcd.cert_dir }}/{{ rusible.host.name }}.crt"),
            "remote_path",
            &context,
        )
        .unwrap();

        assert_eq!(rendered, PathBuf::from("/tmp/certs/web-1.crt"));
    }

    #[test]
    fn template_path_requires_defined_variables() {
        let remote = Remote::new("10.0.0.11", 22, "root", None, None);
        let context = remote.build_context(None, Some("web-1"));

        let error = Field::<PathBuf>::tpl("{{ missing.value }}/server.crt")
            .resolve(&context)
            .unwrap_err();

        assert!(matches!(
            error,
            ResolveValueError::Template(TemplateError::Render { .. })
        ));
    }
}
