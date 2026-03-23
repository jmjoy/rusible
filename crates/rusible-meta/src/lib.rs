//! Serializable task definitions shared between the controller and executor.

mod task;

pub use task::{
	CommandDetails, CommandTask, FileDetails, FileState, FileTask, Task, TaskDetails,
	TaskRequest, TaskResult, TaskSpec, TaskStatus, TemplateDetails, TemplateTask,
};

#[cfg(test)]
mod tests;
