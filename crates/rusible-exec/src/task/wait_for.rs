use crate::Error;
use rusible_meta::task::{
    TaskDetails, TaskResult, TaskStatus,
    wait_for::{WaitForDetails, WaitForTaskData},
};
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};
use tokio::{
    net::{TcpStream, lookup_host},
    time::{sleep, timeout},
};

pub(crate) async fn execute(task: &WaitForTaskData) -> Result<TaskResult, Error> {
    let host = task.host.clone().unwrap_or_else(|| "127.0.0.1".to_string());

    if task.delay_secs > 0 {
        sleep(Duration::from_secs(task.delay_secs)).await;
    }

    let started_at = Instant::now();
    let deadline = started_at + Duration::from_secs(task.timeout_secs);
    let connect_timeout = Duration::from_secs(task.connect_timeout_secs.max(1));
    let mut attempts = 0;

    loop {
        attempts += 1;

        let addrs = lookup_host((host.as_str(), task.port))
            .await?
            .collect::<Vec<SocketAddr>>();
        let connected = connect_any(&addrs, connect_timeout).await;

        let elapsed_ms = started_at.elapsed().as_millis() as u64;
        let details = WaitForDetails {
            host: host.clone(),
            port: task.port,
            attempts,
            elapsed_ms,
            connected,
        };

        if connected {
            return Ok(TaskResult {
                status: TaskStatus::Ok,
                message: Some(format!("{}:{} is reachable", host, task.port)),
                details: Some(TaskDetails::WaitFor(details)),
            });
        }

        if Instant::now() >= deadline {
            return Ok(TaskResult {
                status: TaskStatus::Failed,
                message: Some(
                    Error::WaitForTimeout {
                        host,
                        port: task.port,
                        timeout_secs: task.timeout_secs,
                    }
                    .to_string(),
                ),
                details: Some(TaskDetails::WaitFor(details)),
            });
        }

        sleep(Duration::from_millis(500)).await;
    }
}

async fn connect_any(addrs: &[SocketAddr], connect_timeout: Duration) -> bool {
    for addr in addrs {
        if let Ok(Ok(_)) = timeout(connect_timeout, TcpStream::connect(*addr)).await {
            return true;
        }
    }

    false
}
