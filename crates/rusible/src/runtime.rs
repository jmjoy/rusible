use crate::{Task, TaskResult, TaskStatus};
use russh::{
    ChannelMsg, Disconnect, client,
    keys::{PrivateKeyWithHashAlg, load_secret_key, ssh_key},
};
use russh_sftp::client::SftpSession;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    backtrace::Backtrace,
    collections::{HashMap, HashSet},
    env,
    future::Future,
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{fs, io::AsyncWriteExt, process::Command, task::JoinSet};
use tracing::{debug, info, warn};

/// Executes tasks on a controller target.
pub trait Runnable {
    type Output;
    type InitError;
    type RunError;

    /// Reads a `rusible-exec` binary from disk and prepares it for later task
    /// execution.
    fn init_with_path<P>(
        &mut self,
        exec_path: P,
    ) -> impl Future<Output = Result<(), Self::InitError>> + Send
    where
        Self: Send,
        Self::InitError: From<std::io::Error>,
        P: Into<PathBuf>,
    {
        let exec_path = exec_path.into();

        async move {
            let exec_bytes = fs::read(&exec_path).await?;
            self.init(&exec_bytes).await
        }
    }

    /// Prepares the provided `rusible-exec` binary bytes for later task
    /// execution.
    fn init(
        &mut self,
        exec_bytes: &[u8],
    ) -> impl Future<Output = Result<(), Self::InitError>> + Send
    where
        Self: Send;

    /// Serializes a task, executes it, and returns the structured result.
    fn run<T>(
        &mut self,
        task: T,
    ) -> impl Future<Output = Result<Self::Output, Self::RunError>> + Send
    where
        Self: Send,
        T: Into<Task> + Send;
}

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
    remote_exec_path: Option<String>,
}

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

/// A named host inside an [`Inventory`].
///
/// A host wraps a [`Remote`] target and can belong to multiple groups.
#[derive(Debug, Clone)]
pub struct Host {
    name: String,
    remote: Remote,
    groups: Vec<String>,
}

/// A nested inventory group.
///
/// Groups define hierarchical membership such as `prod -> web`.
#[derive(Debug, Clone, Default)]
pub struct Group {
    name: String,
    groups: Vec<Group>,
}

/// An ansible-like inventory with named hosts and nested groups.
///
/// Hosts are registered separately from groups and declare the groups they
/// belong to. Filtering returns a new inventory with the same group
/// definitions and a narrowed host set.
#[derive(Debug, Clone, Default)]
pub struct Inventory {
    groups: Vec<Group>,
    hosts: Vec<Host>,
}

impl Host {
    /// Creates a named host that points at a remote target.
    pub fn with_remote(name: impl Into<String>, remote: Remote) -> Self {
        Self {
            name: name.into(),
            remote,
            groups: Vec::new(),
        }
    }

    /// Adds a single group membership.
    pub fn add_group(mut self, group: impl Into<String>) -> Self {
        self.groups.push(group.into());
        self
    }

    /// Adds multiple group memberships.
    pub fn add_groups<I, S>(mut self, groups: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.groups.extend(groups.into_iter().map(Into::into));
        self
    }

    /// Returns the inventory host name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the remote target.
    pub fn remote(&self) -> &Remote {
        &self.remote
    }

    /// Returns the declared group memberships.
    pub fn groups(&self) -> &[String] {
        &self.groups
    }
}

impl Group {
    /// Creates an empty group with the provided name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            groups: Vec::new(),
        }
    }

    /// Appends a direct child group.
    pub fn add_group(mut self, group: Group) -> Self {
        self.groups.push(group);
        self
    }

    /// Returns the group name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the direct child groups.
    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    fn collect_descendant_names(&self, names: &mut HashSet<String>) {
        if !names.insert(self.name.clone()) {
            return;
        }

        for group in &self.groups {
            group.collect_descendant_names(names);
        }
    }

    fn collect_group_names_for_match(&self, target: &str, names: &mut HashSet<String>) {
        if self.name == target {
            self.collect_descendant_names(names);
        }

        for group in &self.groups {
            group.collect_group_names_for_match(target, names);
        }
    }
}

impl Inventory {
    /// Creates an empty inventory.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a top-level group.
    pub fn add_group(mut self, group: Group) -> Self {
        self.groups.push(group);
        self
    }

    /// Appends a named host.
    pub fn add_host(mut self, host: Host) -> Self {
        self.hosts.push(host);
        self
    }

    /// Parses an inventory from TOML text.
    pub fn from_toml_str(input: &str) -> Result<Self, InventoryLoadError> {
        let parsed: InventoryToml = toml::from_str(input)?;
        parsed.try_into_inventory()
    }

    /// Parses an inventory from a TOML file on disk.
    pub async fn from_toml_path<P>(path: P) -> Result<Self, InventoryLoadError>
    where
        P: AsRef<Path>,
    {
        let path = path.as_ref();
        let input = fs::read_to_string(path)
            .await
            .map_err(|source| InventoryLoadError::Io {
                path: path.to_path_buf(),
                source,
            })?;

        Self::from_toml_str(&input)
    }

    /// Returns a new `Inventory` containing only hosts reachable through the
    /// matched group and its descendant groups.
    pub fn filter_by_group(&self, group: &str) -> Self {
        let mut group_names = HashSet::new();
        for root in &self.groups {
            root.collect_group_names_for_match(group, &mut group_names);
        }

        Self {
            groups: self.groups.clone(),
            hosts: self
                .hosts
                .iter()
                .filter(|host| host.groups.iter().any(|name| group_names.contains(name)))
                .cloned()
                .collect(),
        }
    }

    /// Returns a new `Inventory` containing only the named host.
    pub fn filter_by_name(&self, name: &str) -> Self {
        self.filter_by_names([name])
    }

    /// Returns a new `Inventory` containing only hosts whose names are in the
    /// provided set.
    pub fn filter_by_names<I, S>(&self, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let names = names.into_iter().map(Into::into).collect::<HashSet<_>>();

        Self {
            groups: self.groups.clone(),
            hosts: self
                .hosts
                .iter()
                .filter(|host| names.contains(host.name()))
                .cloned()
                .collect(),
        }
    }

    /// Returns all top-level groups.
    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    /// Returns all hosts in their declared order.
    pub fn hosts(&self) -> &[Host] {
        &self.hosts
    }

    /// Returns `true` if no hosts are selected.
    pub fn is_empty(&self) -> bool {
        self.hosts.is_empty()
    }

    /// Returns the total number of selected hosts.
    pub fn len(&self) -> usize {
        self.hosts.len()
    }

    fn collect_named_remotes(&self) -> Vec<(String, Remote)> {
        self.hosts
            .iter()
            .map(|host| (host.name.clone(), host.remote.clone()))
            .collect()
    }
}

/// Error returned while loading an [`Inventory`] from TOML.
#[derive(Debug, thiserror::Error)]
pub enum InventoryLoadError {
    #[error("failed to read inventory file {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid inventory TOML: {source}")]
    Toml {
        #[from]
        source: toml::de::Error,
    },

    #[error("duplicate host name `{name}` in inventory")]
    DuplicateHostName { name: String },

    #[error("duplicate group name `{name}` in inventory")]
    DuplicateGroupName { name: String },

    #[error("host `{host}` references unknown group `{group}`")]
    UnknownHostGroup { host: String, group: String },

    #[error("group `{group}` references unknown child group `{child}`")]
    UnknownChildGroup { group: String, child: String },

    #[error("group nesting contains a cycle involving `{group}`")]
    GroupCycle { group: String },
}

#[derive(Debug, Deserialize, Default)]
struct InventoryToml {
    #[serde(default)]
    groups: Vec<InventoryTomlGroup>,
    #[serde(default)]
    hosts: Vec<InventoryTomlHost>,
}

#[derive(Debug, Deserialize)]
struct InventoryTomlGroup {
    name: String,
    #[serde(default)]
    children: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct InventoryTomlHost {
    name: String,
    host: String,
    #[serde(default = "default_remote_port")]
    port: u16,
    user: String,
    password: Option<String>,
    key: Option<PathBuf>,
    #[serde(default)]
    groups: Vec<String>,
}

impl InventoryToml {
    fn try_into_inventory(self) -> Result<Inventory, InventoryLoadError> {
        let mut group_children_by_name = HashMap::new();
        let mut referenced_children = HashSet::new();
        let mut group_names_in_order = Vec::new();

        for group in self.groups {
            if group_children_by_name
                .insert(group.name.clone(), group.children)
                .is_some()
            {
                return Err(InventoryLoadError::DuplicateGroupName { name: group.name });
            }
            group_names_in_order.push(group.name);
        }

        for (group, children) in &group_children_by_name {
            for child in children {
                if !group_children_by_name.contains_key(child) {
                    return Err(InventoryLoadError::UnknownChildGroup {
                        group: group.clone(),
                        child: child.clone(),
                    });
                }
                referenced_children.insert(child.clone());
            }
        }

        let mut built_groups = HashMap::new();
        let mut active_stack = HashSet::new();
        let root_groups = group_names_in_order
            .iter()
            .filter(|name| !referenced_children.contains(*name))
            .map(|name| {
                build_group_tree(
                    name,
                    &group_children_by_name,
                    &mut built_groups,
                    &mut active_stack,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        for name in &group_names_in_order {
            if !built_groups.contains_key(name) {
                build_group_tree(
                    name,
                    &group_children_by_name,
                    &mut built_groups,
                    &mut active_stack,
                )?;
            }
        }

        let mut seen_host_names = HashSet::new();
        let mut hosts = Vec::with_capacity(self.hosts.len());
        for host in self.hosts {
            if !seen_host_names.insert(host.name.clone()) {
                return Err(InventoryLoadError::DuplicateHostName { name: host.name });
            }

            for group in &host.groups {
                if !group_children_by_name.contains_key(group) {
                    return Err(InventoryLoadError::UnknownHostGroup {
                        host: host.name.clone(),
                        group: group.clone(),
                    });
                }
            }

            hosts.push(
                Host::with_remote(
                    host.name,
                    Remote::new(host.host, host.port, host.user, host.password, host.key),
                )
                .add_groups(host.groups),
            );
        }

        Ok(Inventory {
            groups: root_groups,
            hosts,
        })
    }
}

fn build_group_tree(
    name: &str,
    children_by_name: &HashMap<String, Vec<String>>,
    built_groups: &mut HashMap<String, Group>,
    active_stack: &mut HashSet<String>,
) -> Result<Group, InventoryLoadError> {
    if let Some(group) = built_groups.get(name) {
        return Ok(group.clone());
    }

    if !active_stack.insert(name.to_string()) {
        return Err(InventoryLoadError::GroupCycle {
            group: name.to_string(),
        });
    }

    let mut group = Group::new(name.to_string());
    for child in children_by_name.get(name).into_iter().flatten() {
        group = group.add_group(build_group_tree(
            child,
            children_by_name,
            built_groups,
            active_stack,
        )?);
    }

    active_stack.remove(name);
    built_groups.insert(name.to_string(), group.clone());
    Ok(group)
}

fn default_remote_port() -> u16 {
    22
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

trait RunReportLike: Sized {
    fn has_status(&self, status: TaskStatus) -> bool;
}

trait ReportBackedError<R>: Sized {
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
        host: impl Into<String>, port: u16, user: impl Into<String>, password: Option<String>,
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

        // Preserve the input order even though remotes run concurrently.
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

        // Preserve the input order even though hosts run concurrently.
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

async fn run_remote_with_json(
    remote: Remote,
    task_json: String,
) -> Result<RemoteRunReport, RuntimeError> {
    let exec_path = remote
        .remote_exec_path
        .clone()
        .ok_or_else(|| RuntimeError::NotInitialized {
            backtrace: Backtrace::capture(),
        })?;
    let result = match execute_remote_task(&remote, &exec_path, &task_json).await {
        Ok(result) => result,
        Err(error) => TaskResult::unreachable(error.to_string()),
    };

    Ok(RemoteRunReport {
        host: remote.host.clone(),
        exec_path,
        result,
    })
}

async fn run_named_remote_with_json(
    name: String,
    remote: Remote,
    task_json: String,
) -> Result<RemoteRunReport, RuntimeError> {
    let exec_path = remote
        .remote_exec_path
        .clone()
        .ok_or_else(|| RuntimeError::NotInitialized {
            backtrace: Backtrace::capture(),
        })?;
    let result = match execute_remote_task(&remote, &exec_path, &task_json).await {
        Ok(result) => result,
        Err(error) => TaskResult::unreachable(error.to_string()),
    };

    Ok(RemoteRunReport {
        host: name,
        exec_path,
        result,
    })
}

async fn ensure_local_exec(exec_bytes: &[u8]) -> Result<PathBuf, RuntimeError> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| RuntimeError::MissingHome {
            backtrace: Backtrace::capture(),
        })?;
    let hash = embedded_exec_hash(exec_bytes);
    let exec_dir = home.join(".rusible").join("exec").join(hash);
    let exec_path = exec_dir.join("rusible-exec");

    if fs::try_exists(&exec_path).await? {
        debug!(exec_path = %exec_path.display(), "reusing cached local rusible-exec");
        return Ok(exec_path);
    }

    fs::create_dir_all(&exec_dir).await?;
    fs::write(&exec_path, exec_bytes).await?;

    let mut permissions = fs::metadata(&exec_path).await?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&exec_path, permissions).await?;

    debug!(exec_path = %exec_path.display(), "wrote local rusible-exec to cache");

    Ok(exec_path)
}

async fn validate_local_exec(exec_path: &Path) -> Result<(), RuntimeError> {
    let command = format!("{} --version", path_to_string(exec_path));
    let output = Command::new(exec_path).arg("--version").output().await?;
    validate_exec_availability(
        &command,
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
        String::from_utf8_lossy(&output.stderr).trim().to_string(),
    )
}

async fn run_exec_process(exec_path: &Path, task: &Task) -> Result<TaskResult, RuntimeError> {
    let task_json = serde_json::to_string(task)?;
    run_exec_process_with_json(exec_path, &task_json).await
}

async fn run_exec_process_with_json(
    exec_path: &Path, task_json: &str,
) -> Result<TaskResult, RuntimeError> {
    let printable = path_to_string(exec_path);
    debug!(exec_path = %exec_path.display(), payload_bytes = task_json.len(), "spawning local rusible-exec process");
    let mut command = Command::new(exec_path);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn()?;
    let mut stdin = child.stdin.take().ok_or_else(|| RuntimeError::MissingPipe {
        command: printable.clone(),
        stream: "stdin",
        backtrace: Backtrace::capture(),
    })?;
    stdin.write_all(task_json.as_bytes()).await?;
    drop(stdin);

    let output = child.wait_with_output().await?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if stdout.is_empty() {
        return Err(RuntimeError::CommandFailed {
            command: printable,
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            backtrace: Backtrace::capture(),
        });
    }

    let result: TaskResult = serde_json::from_str(&stdout)?;

    if output.status.success()
        || matches!(
            result.status,
            crate::TaskStatus::Failed | crate::TaskStatus::Unreachable
        )
    {
        Ok(result)
    } else {
        Err(RuntimeError::CommandFailed {
            command: printable,
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            backtrace: Backtrace::capture(),
        })
    }
}

async fn initialize_remote_exec(
    remote: &Remote,
    exec_bytes: &[u8],
) -> Result<String, RuntimeError> {
    let mut session = RemoteSession::connect(remote).await?;
    let sftp = session.open_sftp().await?;
    let home = sftp.canonicalize(".").await.map_err(sftp_error)?;
    let install_dir = Path::new(&home)
        .join(".rusible")
        .join("exec")
        .join(embedded_exec_hash(exec_bytes));
    ensure_remote_dir_all(&sftp, &install_dir).await?;

    let exec_path = install_dir.join("rusible-exec");
    let exec_path = path_to_string(&exec_path);
    if !sftp
        .try_exists(exec_path.clone())
        .await
        .map_err(sftp_error)?
    {
        debug!(host = %remote.host, port = remote.port, exec_path = %exec_path, "uploading rusible-exec to remote host");
        let mut file = sftp.create(exec_path.clone()).await.map_err(sftp_error)?;
        file.write_all(exec_bytes).await.map_err(sftp_error)?;
        file.shutdown().await.map_err(sftp_error)?;
    } else {
        debug!(host = %remote.host, port = remote.port, exec_path = %exec_path, "reusing cached remote rusible-exec");
    }
    sftp.close().await.map_err(sftp_error)?;

    session
        .run_simple_command(&format!("chmod 700 {}", shell_quote(&exec_path)))
        .await?;
    session.close().await?;

    Ok(exec_path)
}

async fn validate_remote_exec(remote: &Remote, exec_path: &str) -> Result<(), RuntimeError> {
    let mut session = RemoteSession::connect(remote).await?;
    let command = format!("{} --version", shell_quote(exec_path));
    debug!(host = %remote.host, port = remote.port, exec_path = %exec_path, "validating remote rusible-exec");
    let output = session.run_command(&command, None).await?;
    session.close().await?;

    validate_exec_availability(
        &command,
        output.exit_status.unwrap_or(u32::MAX) as i32,
        output.stdout.trim().to_string(),
        output.stderr.trim().to_string(),
    )
}

async fn execute_remote_task(
    remote: &Remote, exec_path: &str, task_json: &str,
) -> Result<TaskResult, RuntimeError> {
    let mut session = RemoteSession::connect(remote).await?;
    debug!(host = %remote.host, port = remote.port, exec_path = %exec_path, payload_bytes = task_json.len(), "executing remote task");
    let output = session
        .run_command(&shell_quote(exec_path), Some(task_json.as_bytes()))
        .await?;
    session.close().await?;

    let stdout = output.stdout.trim();
    if stdout.is_empty() {
        return Err(RuntimeError::CommandFailed {
            command: exec_path.to_string(),
            status: output.exit_status.unwrap_or(u32::MAX) as i32,
            stderr: output.stderr.trim().to_string(),
            backtrace: Backtrace::capture(),
        });
    }

    let result: TaskResult = serde_json::from_str(stdout)?;
    if output.exit_status == Some(0)
        || matches!(result.status, TaskStatus::Failed | TaskStatus::Unreachable)
    {
        Ok(result)
    } else {
        Err(RuntimeError::CommandFailed {
            command: exec_path.to_string(),
            status: output.exit_status.unwrap_or(u32::MAX) as i32,
            stderr: output.stderr.trim().to_string(),
            backtrace: Backtrace::capture(),
        })
    }
}

async fn ensure_remote_dir_all(sftp: &SftpSession, path: &Path) -> Result<(), RuntimeError> {
    let mut current = PathBuf::new();

    for component in path.components() {
        match component {
            Component::RootDir => current.push(Path::new("/")),
            Component::CurDir => {}
            Component::Normal(part) => {
                current.push(part);
                let current_path = path_to_string(&current);
                if !sftp
                    .try_exists(current_path.clone())
                    .await
                    .map_err(sftp_error)?
                {
                    sftp.create_dir(current_path).await.map_err(sftp_error)?;
                }
            }
            Component::ParentDir | Component::Prefix(_) => {
                return Err(RuntimeError::RemoteProtocol {
                    message: format!("unsupported remote path component in {}", path.display()),
                    backtrace: Backtrace::capture(),
                });
            }
        }
    }

    Ok(())
}

fn embedded_exec_hash(exec_bytes: &[u8]) -> String {
    let digest = Sha256::digest(exec_bytes);
    let mut hash = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hash.push_str(&format!("{byte:02x}"));
    }
    hash
}

fn shell_quote(input: &str) -> String {
    if input.is_empty() {
        return "''".to_string();
    }

    let escaped = input.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn validate_exec_availability(
    command: &str, status: i32, stdout: String, stderr: String,
) -> Result<(), RuntimeError> {
    if status == 0 && !stdout.is_empty() {
        Ok(())
    } else {
        Err(RuntimeError::ExecUnavailable {
            command: command.to_string(),
            status,
            stdout,
            stderr,
            backtrace: Backtrace::capture(),
        })
    }
}

fn sftp_error(error: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::RemoteProtocol {
        message: error.to_string(),
        backtrace: Backtrace::capture(),
    }
}

fn classify_report<R, E>(report: R) -> Result<R, E>
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

struct RemoteCommandOutput {
    stdout: String,
    stderr: String,
    exit_status: Option<u32>,
}

struct RemoteSession {
    handle: client::Handle<SshClient>,
}

struct SshClient;

impl client::Handler for SshClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self, _server_public_key: &ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

impl RemoteSession {
    async fn connect(remote: &Remote) -> Result<Self, RuntimeError> {
        debug!(host = %remote.host, port = remote.port, user = %remote.user, "opening SSH connection");
        let config = Arc::new(client::Config {
            inactivity_timeout: Some(Duration::from_secs(15)),
            ..Default::default()
        });
        let mut handle =
            client::connect(config, (remote.host.as_str(), remote.port), SshClient).await?;

        authenticate_remote(&mut handle, remote).await?;

        debug!(host = %remote.host, port = remote.port, user = %remote.user, "SSH connection authenticated");

        Ok(Self { handle })
    }

    async fn open_sftp(&mut self) -> Result<SftpSession, RuntimeError> {
        let channel = self.handle.channel_open_session().await?;
        channel.request_subsystem(true, "sftp").await?;
        SftpSession::new(channel.into_stream())
            .await
            .map_err(sftp_error)
    }

    async fn run_simple_command(&mut self, command: &str) -> Result<(), RuntimeError> {
        let output = self.run_command(command, None).await?;
        if output.exit_status == Some(0) {
            Ok(())
        } else {
            Err(RuntimeError::CommandFailed {
                command: command.to_string(),
                status: output.exit_status.unwrap_or(u32::MAX) as i32,
                stderr: output.stderr,
                backtrace: Backtrace::capture(),
            })
        }
    }

    async fn run_command(
        &mut self, command: &str, stdin: Option<&[u8]>,
    ) -> Result<RemoteCommandOutput, RuntimeError> {
        let mut channel = self.handle.channel_open_session().await?;
        channel.exec(true, command).await?;

        if let Some(stdin) = stdin {
            let mut writer = channel.make_writer();
            writer.write_all(stdin).await?;
            writer.shutdown().await?;
        }

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_status = None;

        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } => stdout.extend_from_slice(&data),
                ChannelMsg::ExtendedData { data, .. } => stderr.extend_from_slice(&data),
                ChannelMsg::ExitStatus {
                    exit_status: status,
                } => exit_status = Some(status),
                _ => {}
            }
        }

        Ok(RemoteCommandOutput {
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
            exit_status,
        })
    }

    async fn close(&mut self) -> Result<(), RuntimeError> {
        self.handle
            .disconnect(Disconnect::ByApplication, "", "English")
            .await?;
        Ok(())
    }
}

async fn authenticate_remote(
    handle: &mut client::Handle<SshClient>, remote: &Remote,
) -> Result<(), RuntimeError> {
    if let Some(key_path) = &remote.key {
        debug!(host = %remote.host, port = remote.port, user = %remote.user, key_path = %key_path.display(), "authenticating remote with private key");
        let private_key =
            load_secret_key(key_path, None).map_err(|error| RuntimeError::RemoteProtocol {
                message: format!("failed to load private key {}: {error}", key_path.display()),
                backtrace: Backtrace::capture(),
            })?;
        let hash_alg = handle.best_supported_rsa_hash().await?.flatten();
        let auth_result = handle
            .authenticate_publickey(
                remote.user.clone(),
                PrivateKeyWithHashAlg::new(Arc::new(private_key), hash_alg),
            )
            .await?;

        if auth_result.success() {
            return Ok(());
        }

        return Err(RuntimeError::AuthenticationFailed {
            message: format!(
                "public key authentication failed for {}@{}",
                remote.user, remote.host
            ),
            backtrace: Backtrace::capture(),
        });
    }

    if let Some(password) = &remote.password {
        debug!(host = %remote.host, port = remote.port, user = %remote.user, "authenticating remote with password");
        let auth_result = handle
            .authenticate_password(remote.user.clone(), password.clone())
            .await?;

        if auth_result.success() {
            return Ok(());
        }

        return Err(RuntimeError::AuthenticationFailed {
            message: format!(
                "password authentication failed for {}@{}",
                remote.user, remote.host
            ),
            backtrace: Backtrace::capture(),
        });
    }

    Err(RuntimeError::MissingCredentials {
        backtrace: Backtrace::capture(),
    })
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

    fn make_remote(host: &str) -> Remote {
        Remote::new(host, 22, "user", None, None)
    }

    fn make_inventory() -> Inventory {
        Inventory::new()
            .add_group(
                Group::new("prod")
                    .add_group(Group::new("web"))
                    .add_group(Group::new("db")),
            )
            .add_group(Group::new("ops").add_group(Group::new("monitoring")))
            .add_host(Host::with_remote("web-1", make_remote("10.0.0.11")).add_group("web"))
            .add_host(Host::with_remote("db-1", make_remote("10.0.0.21")).add_group("db"))
            .add_host(
                Host::with_remote("bastion-1", make_remote("10.0.0.31"))
                    .add_groups(["web", "monitoring"]),
            )
    }

    const INVENTORY_TOML: &str = r#"
[[groups]]
name = "prod"
children = ["web", "db"]

[[groups]]
name = "ops"
children = ["monitoring"]

[[groups]]
name = "web"

[[groups]]
name = "db"

[[groups]]
name = "monitoring"

[[hosts]]
name = "web-1"
host = "10.0.0.11"
port = 2222
user = "root"
password = "secret"
groups = ["web"]

[[hosts]]
name = "db-1"
host = "10.0.0.21"
user = "root"
key = "/tmp/db.pem"
groups = ["db"]

[[hosts]]
name = "bastion-1"
host = "10.0.0.31"
port = 2224
user = "root"
password = "secret"
groups = ["web", "monitoring"]
"#;

    #[test]
    fn inventory_filter_by_group_includes_descendant_groups() {
        let inventory = make_inventory();

        let prod = inventory.filter_by_group("prod");
        assert_eq!(prod.len(), 3);
        assert_eq!(prod.hosts()[0].name(), "web-1");
        assert_eq!(prod.hosts()[1].name(), "db-1");
        assert_eq!(prod.hosts()[2].name(), "bastion-1");
    }

    #[test]
    fn inventory_filter_by_leaf_group_returns_matching_hosts() {
        let inventory = make_inventory();

        let web = inventory.filter_by_group("web");
        assert_eq!(web.len(), 2);
        assert_eq!(web.hosts()[0].name(), "web-1");
        assert_eq!(web.hosts()[1].name(), "bastion-1");
    }

    #[test]
    fn inventory_filter_by_name_returns_single_host() {
        let inventory = make_inventory();

        let host = inventory.filter_by_name("db-1");
        assert_eq!(host.len(), 1);
        assert_eq!(host.hosts()[0].name(), "db-1");
    }

    #[test]
    fn inventory_filter_by_names_returns_multiple_hosts() {
        let inventory = make_inventory();

        let selected = inventory.filter_by_names(["web-1", "bastion-1"]);
        assert_eq!(selected.len(), 2);
        assert_eq!(selected.hosts()[0].name(), "web-1");
        assert_eq!(selected.hosts()[1].name(), "bastion-1");
    }

    #[test]
    fn inventory_chained_filters_intersect_host_sets() {
        let inventory = make_inventory();

        let selected = inventory.filter_by_group("ops").filter_by_name("bastion-1");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected.hosts()[0].name(), "bastion-1");
    }

    #[test]
    fn inventory_len_counts_hosts() {
        let inventory = make_inventory();

        assert_eq!(inventory.len(), 3);
        assert!(!inventory.is_empty());
    }

    #[test]
    fn inventory_filter_returns_empty_when_no_match() {
        let inventory = make_inventory();

        assert!(inventory.filter_by_group("missing").is_empty());
        assert!(inventory.filter_by_name("missing").is_empty());
    }

    #[test]
    fn inventory_parses_from_toml() {
        let inventory = Inventory::from_toml_str(INVENTORY_TOML).unwrap();

        assert_eq!(inventory.groups().len(), 2);
        assert_eq!(inventory.groups()[0].name(), "prod");
        assert_eq!(inventory.groups()[0].groups()[0].name(), "web");
        assert_eq!(inventory.hosts().len(), 3);
        assert_eq!(inventory.hosts()[0].name(), "web-1");
        assert_eq!(inventory.hosts()[0].remote().port, 2222);
        assert_eq!(inventory.hosts()[1].remote().port, 22);
    }

    #[test]
    fn inventory_parse_rejects_unknown_host_group() {
        let inventory_toml = r#"
[[groups]]
name = "web"

[[hosts]]
name = "web-1"
host = "127.0.0.1"
user = "root"
groups = ["missing"]
"#;

        assert!(matches!(
            Inventory::from_toml_str(inventory_toml),
            Err(InventoryLoadError::UnknownHostGroup { .. })
        ));
    }

    #[test]
    fn inventory_parse_rejects_group_cycles() {
        let inventory_toml = r#"
[[groups]]
name = "prod"
children = ["web"]

[[groups]]
name = "web"
children = ["prod"]
"#;

        assert!(matches!(
            Inventory::from_toml_str(inventory_toml),
            Err(InventoryLoadError::GroupCycle { .. })
        ));
    }

    #[test]
    fn hash_is_stable() {
        assert_eq!(embedded_exec_hash(b"abc"), embedded_exec_hash(b"abc"));
    }

    #[test]
    fn local_exec_path_uses_hash() {
        let hash = embedded_exec_hash(b"abc");
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("/tmp/it's ok"), "'/tmp/it'\"'\"'s ok'");
    }

    #[test]
    fn exec_availability_accepts_version_output() {
        assert!(validate_exec_availability(
            "rusible-exec --version",
            0,
            "rusible-exec 0.1.0".to_string(),
            String::new(),
        )
        .is_ok());
    }

    #[test]
    fn exec_availability_rejects_empty_output() {
        assert!(matches!(
            validate_exec_availability("rusible-exec --version", 0, String::new(), String::new()),
            Err(RuntimeError::ExecUnavailable { .. })
        ));
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
