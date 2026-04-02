use super::file;
use crate::Error;
use rusible_meta::{DownloadDetails, DownloadTaskData, TaskDetails, TaskResult, TaskStatus};
use tokio::fs;

pub(crate) async fn execute(task: &DownloadTaskData) -> Result<TaskResult, Error> {
    let dest = task.dest.clone();

    if let Some(parent) = dest.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).await?;
    }

    let destination_exists = fs::try_exists(&dest).await?;
    let mut downloaded = false;
    let mut bytes_written = 0;

    if task.force || !destination_exists {
        let response = reqwest::get(task.url.clone()).await?.error_for_status()?;
        let body = response.bytes().await?;
        bytes_written = body.len() as u64;

        let temp_path = temporary_download_path(&dest);
        fs::write(&temp_path, &body).await?;
        fs::rename(&temp_path, &dest).await?;
        downloaded = true;
    }

    let mode_changed = file::apply_mode(&dest, task.mode.as_deref()).await?;
    let ownership_changed =
        file::apply_owner_group(&dest, task.owner.as_deref(), task.group.as_deref()).await?;
    let details = DownloadDetails {
        url: task.url.clone(),
        dest,
        downloaded,
        bytes_written,
        mode_changed,
        ownership_changed,
    };
    let changed = downloaded || mode_changed || ownership_changed;

    Ok(TaskResult {
        status: if changed {
            TaskStatus::Changed
        } else {
            TaskStatus::Ok
        },
        message: Some(if downloaded {
            format!("downloaded {} to {}", task.url, details.dest.display())
        } else {
            format!("{} already exists", details.dest.display())
        }),
        details: Some(TaskDetails::Download(details)),
    })
}

fn temporary_download_path(dest: &std::path::Path) -> std::path::PathBuf {
    let file_name = dest
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "download".to_string());

    dest.with_file_name(format!(".{file_name}.rusible-download"))
}

#[cfg(test)]
mod tests {
    use super::execute;
    use rusible_meta::{DownloadDetails, DownloadTaskData, TaskDetails, TaskStatus};
    use std::{
        env, fs,
        io::{Read, Write},
        net::TcpListener,
        path::PathBuf,
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };
    use url::Url;

    #[tokio::test(flavor = "current_thread")]
    async fn download_task_fetches_content() {
        let (url, server) = spawn_http_server(b"hello world");
        let destination = unique_temp_path("downloaded");

        let result = execute(&DownloadTaskData {
            name: None,
            url: Url::parse(&url).unwrap(),
            dest: destination.clone(),
            force: false,
            owner: None,
            group: None,
            mode: None,
        })
        .await
        .unwrap();

        server.join().unwrap();

        assert_eq!(result.status, TaskStatus::Changed);
        assert_eq!(fs::read_to_string(&destination).unwrap(), "hello world");
        assert!(matches!(
            result.details,
            Some(TaskDetails::Download(DownloadDetails {
                downloaded: true,
                bytes_written: 11,
                ..
            }))
        ));

        fs::remove_file(destination).unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn download_task_writes_to_resolved_destination() {
        let (url, server) = spawn_http_server(b"templated");
        let destination = unique_temp_path("templated");

        let result = execute(&DownloadTaskData {
            name: None,
            url: Url::parse(&url).unwrap(),
            dest: destination.clone(),
            force: false,
            owner: None,
            group: None,
            mode: None,
        })
        .await
        .unwrap();

        server.join().unwrap();

        assert_eq!(result.status, TaskStatus::Changed);
        assert_eq!(fs::read_to_string(&destination).unwrap(), "templated");
        assert!(matches!(
            result.details,
            Some(TaskDetails::Download(DownloadDetails {
                downloaded: true,
                bytes_written: 9,
                ..
            }))
        ));

        fs::remove_file(destination).unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn download_task_skips_existing_file_when_not_forced() {
        let destination = unique_temp_path("existing");
        fs::write(&destination, "existing").unwrap();

        let result = execute(&DownloadTaskData {
            name: None,
            url: Url::parse("http://127.0.0.1:9/unused").unwrap(),
            dest: destination.clone(),
            force: false,
            owner: None,
            group: None,
            mode: None,
        })
        .await
        .unwrap();

        assert_eq!(result.status, TaskStatus::Ok);
        assert_eq!(fs::read_to_string(&destination).unwrap(), "existing");

        fs::remove_file(destination).unwrap();
    }

    fn spawn_http_server(body: &'static [u8]) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            let _ = stream.read(&mut request).unwrap();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
        });

        (format!("http://{address}/artifact"), server)
    }

    fn unique_temp_path(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("rusible-download-{prefix}-{stamp}"))
    }
}
