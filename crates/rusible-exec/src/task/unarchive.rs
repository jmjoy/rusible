use crate::Error;
use rusible_meta::{TaskDetails, TaskResult, TaskStatus, UnarchiveDetails, UnarchiveTaskData};
use std::path::Path;
use tokio::{fs, process::Command};

pub(crate) async fn execute(task: &UnarchiveTaskData) -> Result<TaskResult, Error> {
    if let Some(creates) = task.creates.as_deref()
        && fs::try_exists(creates).await? {
            return Ok(TaskResult {
                status: TaskStatus::Skipped,
                message: Some(format!(
                    "archive {} not extracted because {} already exists",
                    task.src.display(),
                    creates.display()
                )),
                details: Some(TaskDetails::Unarchive(UnarchiveDetails {
                    src: task.src.clone(),
                    dest: task.dest.clone(),
                    extracted: false,
                    creates: task.creates.clone(),
                })),
            });
        }

    fs::create_dir_all(&task.dest).await?;

    let flag = extract_flag(&task.src);
    let mut command = Command::new("tar");
    command.arg(flag).arg(&task.src).arg("-C").arg(&task.dest);
    let output = command.output().await?;
    if !output.status.success() {
        return Err(Error::CommandFailed {
            program: format!(
                "tar {flag} {} -C {}",
                task.src.display(),
                task.dest.display()
            ),
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    Ok(TaskResult {
        status: TaskStatus::Changed,
        message: Some(format!(
            "extracted {} into {}",
            task.src.display(),
            task.dest.display()
        )),
        details: Some(TaskDetails::Unarchive(UnarchiveDetails {
            src: task.src.clone(),
            dest: task.dest.clone(),
            extracted: true,
            creates: task.creates.clone(),
        })),
    })
}

fn extract_flag(path: &Path) -> &'static str {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        "-xzf"
    } else if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") {
        "-xjf"
    } else if name.ends_with(".tar.xz") || name.ends_with(".txz") {
        "-xJf"
    } else {
        "-xf"
    }
}
