mod command;
mod file;
mod template;

use crate::Error;
use rusible_meta::{Task, TaskRequest, TaskResult};

pub(crate) fn execute(request: TaskRequest) -> Result<TaskResult, Error> {
    match request.task {
        Task::File(task) => file::execute(&task),
        Task::Template(task) => template::execute(&task, &request.context),
        Task::Command(task) => command::execute(&task),
    }
}
