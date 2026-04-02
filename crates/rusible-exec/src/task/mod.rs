mod command;
mod copy;
mod download;
mod file;
mod shell;
mod stat;
mod systemd;
mod unarchive;
mod user;
mod wait_for;

use crate::Error;
use rusible_meta::{TaskData, TaskRequest, TaskResult};

pub(crate) async fn execute(request: TaskRequest) -> Result<TaskResult, Error> {
    request.task.validate()?;

    match request.task {
        TaskData::File(task) => file::execute(&task).await,
        TaskData::Command(task) => command::execute(&task).await,
        TaskData::Copy(task) => copy::execute(&task).await,
        TaskData::Download(task) => download::execute(&task).await,
        TaskData::Shell(task) => shell::execute(&task).await,
        TaskData::Stat(task) => stat::execute(&task).await,
        TaskData::User(task) => user::execute(&task).await,
        TaskData::Systemd(task) => systemd::execute(&task).await,
        TaskData::Unarchive(task) => unarchive::execute(&task).await,
        TaskData::WaitFor(task) => wait_for::execute(&task).await,
    }
}
