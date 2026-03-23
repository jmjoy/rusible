//! Serializable task definitions shared between the controller and executor.

mod task;

pub use rusible_template::{TemplatedPath, TemplatedString, TemplatedUrl};
pub use task::{
	DownloadDetails, DownloadTask,
	CommandDetails, CommandTask, CopyDetails, CopyTask, FileDetails, FileState,
	FileTask, ShellDetails, ShellTask, StatDetails, StatTask, SystemdDetails,
	SystemdState, SystemdTask, Task, TaskDetails, TaskRequest, TaskResult, TaskSpec,
	TaskStatus, TemplateDetails, TemplateTask, UnarchiveDetails, UnarchiveTask,
	UserDetails, UserTask, WaitForDetails, WaitForTask,
};

#[cfg(test)]
mod tests;
