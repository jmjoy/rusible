#![feature(error_generic_member_access)]

//! Controller-side runtime for executing Rusible tasks locally or over SSH.

mod exec;
pub mod inventory;
pub mod report;
pub mod runtime;
pub mod target;

pub use rusible_meta as meta;
