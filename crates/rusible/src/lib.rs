#![feature(error_generic_member_access)]

//! Controller-side runtime for executing Rusible tasks locally or over SSH.

mod exec;
pub mod inventory;
pub mod logging;
pub mod report;
pub mod runtime;
pub mod shell;
pub mod target;
pub mod vars;

pub use logging::init_forest_logging;
pub use rusible_meta as meta;
