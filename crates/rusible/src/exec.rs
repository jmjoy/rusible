use crate::{
    meta::{TaskRequest, TaskResult, TaskStatus},
    report::{RemoteRunReport, RuntimeError},
    target::{Remote, UploadOptions},
};
use russh::{
    ChannelMsg, Disconnect, client,
    keys::{PrivateKeyWithHashAlg, load_secret_key, ssh_key},
};
use russh_sftp::client::SftpSession;
use sha2::{Digest, Sha256};
use std::{
    backtrace::Backtrace,
    env,
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{fs, io::AsyncWriteExt, process::Command};
use tracing::debug;

pub(crate) async fn run_remote_with_json(
    remote: Remote, task_json: String,
) -> Result<RemoteRunReport, RuntimeError> {
    let exec_path =
        remote
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

pub(crate) async fn run_named_remote_with_json(
    name: String, remote: Remote, task_json: String,
) -> Result<RemoteRunReport, RuntimeError> {
    let exec_path =
        remote
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

pub(crate) async fn ensure_local_exec(exec_bytes: &[u8]) -> Result<PathBuf, RuntimeError> {
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

pub(crate) async fn validate_local_exec(exec_path: &Path) -> Result<(), RuntimeError> {
    let command = format!("{} --version", path_to_string(exec_path));
    let output = Command::new(exec_path).arg("--version").output().await?;
    validate_exec_availability(
        &command,
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
        String::from_utf8_lossy(&output.stderr).trim().to_string(),
    )
}

pub(crate) async fn run_exec_process(
    exec_path: &Path, request: &TaskRequest,
) -> Result<TaskResult, RuntimeError> {
    let task_json = serde_json::to_string(request)?;
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
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| RuntimeError::MissingPipe {
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
        || matches!(result.status, TaskStatus::Failed | TaskStatus::Unreachable)
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

pub(crate) async fn initialize_remote_exec(
    remote: &Remote, exec_bytes: &[u8],
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

pub(crate) async fn upload_remote_bytes(
    remote: &Remote, remote_path: &Path, bytes: &[u8], options: &UploadOptions,
) -> Result<String, RuntimeError> {
    let mut session = RemoteSession::connect(remote).await?;
    let sftp = session.open_sftp().await?;

    if let Some(parent) = remote_path.parent() {
        ensure_remote_dir_all(&sftp, parent).await?;
    }

    let remote_path = path_to_string(remote_path);
    debug!(
        host = %remote.host,
        port = remote.port,
        remote_path = %remote_path,
        bytes = bytes.len(),
        "uploading local file to remote host"
    );

    let mut file = sftp.create(remote_path.clone()).await.map_err(sftp_error)?;
    file.write_all(bytes).await.map_err(sftp_error)?;
    file.shutdown().await.map_err(sftp_error)?;
    sftp.close().await.map_err(sftp_error)?;

    apply_remote_upload_options(&mut session, &remote_path, options).await?;
    session.close().await?;

    Ok(remote_path)
}

async fn apply_remote_upload_options(
    session: &mut RemoteSession, remote_path: &str, options: &UploadOptions,
) -> Result<(), RuntimeError> {
    if let Some(mode) = &options.mode {
        session
            .run_simple_command(&format!(
                "chmod {} {}",
                shell_quote(mode),
                shell_quote(remote_path)
            ))
            .await?;
    }

    match (&options.owner, &options.group) {
        (Some(owner), Some(group)) => {
            session
                .run_simple_command(&format!(
                    "chown {}:{} {}",
                    shell_quote(owner),
                    shell_quote(group),
                    shell_quote(remote_path)
                ))
                .await?;
        }
        (Some(owner), None) => {
            session
                .run_simple_command(&format!(
                    "chown {} {}",
                    shell_quote(owner),
                    shell_quote(remote_path)
                ))
                .await?;
        }
        (None, Some(group)) => {
            session
                .run_simple_command(&format!(
                    "chgrp {} {}",
                    shell_quote(group),
                    shell_quote(remote_path)
                ))
                .await?;
        }
        (None, None) => {}
    }

    Ok(())
}

pub(crate) async fn validate_remote_exec(
    remote: &Remote, exec_path: &str,
) -> Result<(), RuntimeError> {
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

pub(crate) async fn execute_remote_task(
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

pub(crate) fn embedded_exec_hash(exec_bytes: &[u8]) -> String {
    let digest = Sha256::digest(exec_bytes);
    let mut hash = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hash.push_str(&format!("{byte:02x}"));
    }
    hash
}

pub(crate) fn shell_quote(input: &str) -> String {
    if input.is_empty() {
        return "''".to_string();
    }

    let escaped = input.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub(crate) fn validate_exec_availability(
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
        assert!(
            validate_exec_availability(
                "rusible-exec --version",
                0,
                "rusible-exec 0.1.0".to_string(),
                String::new(),
            )
            .is_ok()
        );
    }

    #[test]
    fn exec_availability_rejects_empty_output() {
        assert!(matches!(
            validate_exec_availability("rusible-exec --version", 0, String::new(), String::new()),
            Err(RuntimeError::ExecUnavailable { .. })
        ));
    }
}
