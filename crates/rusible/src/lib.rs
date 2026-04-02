#![feature(error_generic_member_access)]

//! Controller-side runtime for executing Rusible tasks locally or over SSH.

mod exec;
pub mod inventory;
pub mod logging;
pub mod report;
pub mod runtime;
mod shell;
pub mod target;
mod vars;

pub use logging::init_forest_logging;
pub use rusible_meta as meta;
pub use rusible_template::{Field, ResolveTemplate, TemplatedPath, TemplatedString, TemplatedUrl};
pub use shell::{QuoteError, shell_quote, shell_quote_path};
pub use target::UploadOptions;
/// Re-exported TOML table type for task variable maps.
pub use toml::Table;
/// Re-exported TOML value type for task variable values.
pub use toml::Value;
/// Re-exported TOML macro for constructing task variables.
pub use toml::toml;
/// Re-exported TOML array type for task variable lists.
pub use toml::value::Array;
pub use vars::{VarError, VarLookupError};
