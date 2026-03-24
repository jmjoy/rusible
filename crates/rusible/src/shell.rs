use std::path::Path;

pub use shlex::QuoteError;

/// Quotes a single shell word for POSIX-compatible shells.
pub fn shell_quote(input: impl AsRef<str>) -> Result<String, QuoteError> {
    shlex::try_quote(input.as_ref()).map(|quoted| quoted.into_owned())
}

/// Quotes a filesystem path as a single shell word.
pub fn shell_quote_path(path: impl AsRef<Path>) -> Result<String, QuoteError> {
    shlex::try_quote(&path.as_ref().to_string_lossy()).map(|quoted| quoted.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("/tmp/it's ok").unwrap(), "\"/tmp/it's ok\"");
    }

    #[test]
    fn shell_quote_handles_empty_strings() {
        assert_eq!(shell_quote("").unwrap(), "''");
    }
}
