mod command;
mod copy;
mod download;
mod file;
mod shell;
mod stat;
mod systemd;
mod template;
mod unarchive;
mod user;
mod wait_for;

use crate::Error;
use rusible_meta::{Task, TaskRequest, TaskResult};

pub(crate) async fn execute(request: TaskRequest) -> Result<TaskResult, Error> {
    match request.task {
        Task::File(task) => file::execute(&task).await,
        Task::Template(task) => template::execute(&task, &request.context).await,
        Task::Command(task) => command::execute(&task).await,
        Task::Copy(task) => copy::execute(&task).await,
        Task::Download(task) => download::execute(&task).await,
        Task::Shell(task) => shell::execute(&task).await,
        Task::Stat(task) => stat::execute(&task).await,
        Task::User(task) => user::execute(&task).await,
        Task::Systemd(task) => systemd::execute(&task).await,
        Task::Unarchive(task) => unarchive::execute(&task).await,
        Task::WaitFor(task) => wait_for::execute(&task).await,
    }
}
