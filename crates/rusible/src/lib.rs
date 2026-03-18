#![feature(error_generic_member_access)]

//! Controller-side runtime for executing Rusible tasks locally or over SSH.

mod runtime;

pub use runtime::{
    BatchRunError, BatchRunReport, Group, Host, Inventory, Local, LocalRunError,
    InventoryLoadError, LocalRunReport, Remote, RemoteRunError, RemoteRunReport,
    RunResultExt, Runnable, RuntimeError,
};
pub use rusible_meta::{FileState, FileTask, Task, TaskResult, TaskStatus, TemplateTask};
