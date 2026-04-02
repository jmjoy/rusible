//! Serializable task definitions shared between the controller and executor.

pub mod field;
mod task;

pub use field::{Field, ResolveValue, ResolveValueError, TemplateError, render_string};
pub use task::{
    CommandDetails, CommandTask, CommandTaskData, CopyDetails, CopyTask, CopyTaskData,
    DownloadDetails, DownloadTask, DownloadTaskData, FileDetails, FileState, FileTask,
    FileTaskData, ShellDetails, ShellTask, ShellTaskData, StatDetails, StatTask, StatTaskData,
    SystemdDetails, SystemdState, SystemdTask, SystemdTaskData, Task, TaskData, TaskDataSpec,
    TaskDetails, TaskRequest, TaskResult, TaskSpec, TaskStatus, TaskValidationError,
    UnarchiveDetails, UnarchiveTask, UnarchiveTaskData, UserDetails, UserTask, UserTaskData,
    WaitForDetails, WaitForTask, WaitForTaskData,
};

#[cfg(test)]
mod tests;
