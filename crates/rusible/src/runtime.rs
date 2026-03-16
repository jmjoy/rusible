use crate::{Task, TaskResult, TaskStatus};
use russh::{
    ChannelMsg, Disconnect, client,
    keys::{PrivateKeyWithHashAlg, load_secret_key, ssh_key},
};
use russh_sftp::client::SftpSession;
use sha2::{Digest, Sha256};
use std::{
    backtrace::Backtrace,
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
    type Error;

    /// Reads a `rusible-exec` binary from disk and prepares it for later task
    /// execution.
    fn init<P>(&mut self, exec_path: P) -> impl Future<Output = Result<(), Self::Error>> + Send
    where
        Self: Send,
        Self::Error: From<std::io::Error>,
        P: Into<PathBuf>,
    {
        let exec_path = exec_path.into();

        async move {
            let exec_bytes = fs::read(&exec_path).await?;
            self.init_with_bytes(&exec_bytes).await
        }
    }

    /// Prepares the provided `rusible-exec` binary bytes for later task
    /// execution.
    fn init_with_bytes(
        &mut self,
        exec_bytes: &[u8],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send
    where
        Self: Send;

    /// Serializes a task, executes it, and returns the structured result.
    fn run<T>(&mut self, task: T) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send
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
#[derive(Debug, Clone)]
pub struct LocalRunReport {
    pub exec_path: PathBuf,
    pub result: TaskResult,
}

/// Result of a task run on a remote host.
#[derive(Debug, Clone)]
pub struct RemoteRunReport {
    pub host: String,
    pub exec_path: String,
    pub result: TaskResult,
}

/// Aggregated results from multiple remote hosts.
#[derive(Debug, Clone)]
pub struct BatchRunReport {
    pub results: Vec<RemoteRunReport>,
}

/// Error type shared by the runtime implementations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
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
    type Error = Error;
    type Output = LocalRunReport;

    async fn init_with_bytes(&mut self, exec_bytes: &[u8]) -> Result<(), Self::Error> {
        let exec_path = ensure_local_exec(exec_bytes).await?;
        validate_local_exec(&exec_path).await?;
        info!(exec_path = %exec_path.display(), "initialized local rusible-exec");
        self.exec_path = Some(exec_path);
        Ok(())
    }

    async fn run<T>(&mut self, task: T) -> Result<Self::Output, Self::Error>
    where
        T: Into<Task> + Send,
    {
        let task = task.into();
        let exec_path = self.exec_path.clone().ok_or_else(|| Error::NotInitialized {
            backtrace: Backtrace::capture(),
        })?;
        debug!(exec_path = %exec_path.display(), task = ?task, "running task locally");
        let result = run_exec_process(&exec_path, &task).await?;
        info!(exec_path = %exec_path.display(), status = ?result.status, "local task finished");

        Ok(LocalRunReport { exec_path, result })
    }
}

impl Runnable for Remote {
    type Error = Error;
    type Output = RemoteRunReport;

    async fn init_with_bytes(&mut self, exec_bytes: &[u8]) -> Result<(), Self::Error> {
        let remote_exec_path = initialize_remote_exec(self, exec_bytes).await?;
        validate_remote_exec(self, &remote_exec_path).await?;
        info!(host = %self.host, port = self.port, exec_path = %remote_exec_path, "initialized remote rusible-exec");
        self.remote_exec_path = Some(remote_exec_path);
        Ok(())
    }

    async fn run<T>(&mut self, task: T) -> Result<Self::Output, Self::Error>
    where
        T: Into<Task> + Send,
    {
        let task = task.into();
        let exec_path = self.remote_exec_path.clone().ok_or_else(|| Error::NotInitialized {
            backtrace: Backtrace::capture(),
        })?;
        debug!(host = %self.host, port = self.port, exec_path = %exec_path, task = ?task, "running task on remote host");
        let task_json = serde_json::to_string(&task)?;
        let result = match execute_remote_task(self, &exec_path, &task_json).await {
            Ok(result) => result,
            Err(error) => {
                warn!(host = %self.host, port = self.port, exec_path = %exec_path, error = %error, "remote task became unreachable");
                TaskResult::unreachable(error.to_string())
            }
        };
        info!(host = %self.host, port = self.port, exec_path = %exec_path, status = ?result.status, "remote task finished");

        Ok(RemoteRunReport {
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
    type Error = Error;
    type Output = BatchRunReport;

    async fn init_with_bytes(&mut self, exec_bytes: &[u8]) -> Result<(), Self::Error> {
        let remotes = self.clone().into_iter().collect::<Vec<_>>();
        info!(remote_count = remotes.len(), "initializing remote executables for batch");

        let mut initialized_remotes = Vec::new();

        for mut remote in remotes {
            remote.init_with_bytes(exec_bytes).await?;
            initialized_remotes.push(remote);
        }

        *self = initialized_remotes.into_iter().collect();
        Ok(())
    }

    async fn run<T>(&mut self, task: T) -> Result<Self::Output, Self::Error>
    where
        T: Into<Task> + Send,
    {
        let task = task.into();
        let task_json = serde_json::to_string(&task)?;
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
            let (index, report) = joined?;
            results[index] = Some(report?);
        }

        let results = results
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .expect("all spawned remote runs should produce a result");

        info!(remote_count = results.len(), "batch task finished");

        Ok(BatchRunReport { results })
    }
}

async fn run_remote_with_json(remote: Remote, task_json: String) -> Result<RemoteRunReport, Error> {
    let exec_path = remote
        .remote_exec_path
        .clone()
        .ok_or_else(|| Error::NotInitialized {
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

async fn ensure_local_exec(exec_bytes: &[u8]) -> Result<PathBuf, Error> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| Error::MissingHome {
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

async fn validate_local_exec(exec_path: &Path) -> Result<(), Error> {
    let command = format!("{} --version", path_to_string(exec_path));
    let output = Command::new(exec_path).arg("--version").output().await?;
    validate_exec_availability(
        &command,
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
        String::from_utf8_lossy(&output.stderr).trim().to_string(),
    )
}

async fn run_exec_process(exec_path: &Path, task: &Task) -> Result<TaskResult, Error> {
    let task_json = serde_json::to_string(task)?;
    run_exec_process_with_json(exec_path, &task_json).await
}

async fn run_exec_process_with_json(
    exec_path: &Path, task_json: &str,
) -> Result<TaskResult, Error> {
    let printable = path_to_string(exec_path);
    debug!(exec_path = %exec_path.display(), payload_bytes = task_json.len(), "spawning local rusible-exec process");
    let mut command = Command::new(exec_path);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn()?;
    let mut stdin = child.stdin.take().ok_or_else(|| Error::MissingPipe {
        command: printable.clone(),
        stream: "stdin",
        backtrace: Backtrace::capture(),
    })?;
    stdin.write_all(task_json.as_bytes()).await?;
    drop(stdin);

    let output = child.wait_with_output().await?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if stdout.is_empty() {
        return Err(Error::CommandFailed {
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
        Err(Error::CommandFailed {
            command: printable,
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            backtrace: Backtrace::capture(),
        })
    }
}

async fn initialize_remote_exec(remote: &Remote, exec_bytes: &[u8]) -> Result<String, Error> {
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

async fn validate_remote_exec(remote: &Remote, exec_path: &str) -> Result<(), Error> {
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
) -> Result<TaskResult, Error> {
    let mut session = RemoteSession::connect(remote).await?;
    debug!(host = %remote.host, port = remote.port, exec_path = %exec_path, payload_bytes = task_json.len(), "executing remote task");
    let output = session
        .run_command(&shell_quote(exec_path), Some(task_json.as_bytes()))
        .await?;
    session.close().await?;

    let stdout = output.stdout.trim();
    if stdout.is_empty() {
        return Err(Error::CommandFailed {
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
        Err(Error::CommandFailed {
            command: exec_path.to_string(),
            status: output.exit_status.unwrap_or(u32::MAX) as i32,
            stderr: output.stderr.trim().to_string(),
            backtrace: Backtrace::capture(),
        })
    }
}

async fn ensure_remote_dir_all(sftp: &SftpSession, path: &Path) -> Result<(), Error> {
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
                return Err(Error::RemoteProtocol {
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
) -> Result<(), Error> {
    if status == 0 && !stdout.is_empty() {
        Ok(())
    } else {
        Err(Error::ExecUnavailable {
            command: command.to_string(),
            status,
            stdout,
            stderr,
            backtrace: Backtrace::capture(),
        })
    }
}

fn sftp_error(error: impl std::fmt::Display) -> Error {
    Error::RemoteProtocol {
        message: error.to_string(),
        backtrace: Backtrace::capture(),
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
    async fn connect(remote: &Remote) -> Result<Self, Error> {
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

    async fn open_sftp(&mut self) -> Result<SftpSession, Error> {
        let channel = self.handle.channel_open_session().await?;
        channel.request_subsystem(true, "sftp").await?;
        SftpSession::new(channel.into_stream())
            .await
            .map_err(sftp_error)
    }

    async fn run_simple_command(&mut self, command: &str) -> Result<(), Error> {
        let output = self.run_command(command, None).await?;
        if output.exit_status == Some(0) {
            Ok(())
        } else {
            Err(Error::CommandFailed {
                command: command.to_string(),
                status: output.exit_status.unwrap_or(u32::MAX) as i32,
                stderr: output.stderr,
                backtrace: Backtrace::capture(),
            })
        }
    }

    async fn run_command(
        &mut self, command: &str, stdin: Option<&[u8]>,
    ) -> Result<RemoteCommandOutput, Error> {
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

    async fn close(&mut self) -> Result<(), Error> {
        self.handle
            .disconnect(Disconnect::ByApplication, "", "English")
            .await?;
        Ok(())
    }
}

async fn authenticate_remote(
    handle: &mut client::Handle<SshClient>, remote: &Remote,
) -> Result<(), Error> {
    if let Some(key_path) = &remote.key {
        debug!(host = %remote.host, port = remote.port, user = %remote.user, key_path = %key_path.display(), "authenticating remote with private key");
        let private_key =
            load_secret_key(key_path, None).map_err(|error| Error::RemoteProtocol {
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

        return Err(Error::AuthenticationFailed {
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

        return Err(Error::AuthenticationFailed {
            message: format!(
                "password authentication failed for {}@{}",
                remote.user, remote.host
            ),
            backtrace: Backtrace::capture(),
        });
    }

    Err(Error::MissingCredentials {
        backtrace: Backtrace::capture(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
            Err(Error::ExecUnavailable { .. })
        ));
    }
}
