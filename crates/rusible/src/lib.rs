#![feature(error_generic_member_access)]

//! Controller-side runtime for executing Rusible tasks locally or over SSH.

mod runtime;

pub use runtime::{
    BatchRunReport, Error, Local, LocalRunReport, Remote, RemoteRunReport, Runnable,
};
pub use rusible_meta::{FileState, FileTask, Task, TaskResult, TaskStatus, TemplateTask};
