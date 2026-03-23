#![feature(error_generic_member_access)]

//! Controller-side runtime for executing Rusible tasks locally or over SSH.

mod exec;
pub mod inventory;
pub mod report;
pub mod runtime;
pub mod target;
mod vars;

pub use rusible_meta as meta;
pub use target::{TemplatePath, UploadOptions};
/// Re-exported TOML table type for task variable maps.
pub use toml::Table;
/// Re-exported TOML value type for task variable values.
pub use toml::Value;
/// Re-exported TOML macro for constructing task variables.
pub use toml::toml;
/// Re-exported TOML array type for task variable lists.
pub use toml::value::Array;
pub use vars::{VarError, VarLookupError};
