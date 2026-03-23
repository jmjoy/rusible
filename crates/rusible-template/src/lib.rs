use minijinja::{Environment, UndefinedBehavior};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use toml::Table;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TemplateError {
    #[error("template rendering failed: {message}")]
    Render { message: String },
}

pub trait ResolveTemplate {
    type Output;

    fn resolve(&self, context: &Table) -> Result<Self::Output, TemplateError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TemplatedPath(String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TemplatedString(String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TemplatedUrl(String);

impl TemplatedPath {
    pub fn new(template: impl Into<String>) -> Self {
        Self(template.into())
    }

    pub fn literal(path: impl AsRef<Path>) -> Self {
        Self(path.as_ref().to_string_lossy().into_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TemplatedString {
    pub fn new(template: impl Into<String>) -> Self {
        Self(template.into())
    }

    pub fn literal(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TemplatedUrl {
    pub fn new(template: impl Into<String>) -> Self {
        Self(template.into())
    }

    pub fn literal(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl ResolveTemplate for TemplatedPath {
    type Output = PathBuf;

    fn resolve(&self, context: &Table) -> Result<Self::Output, TemplateError> {
        Ok(PathBuf::from(render_string(&self.0, context)?))
    }
}

impl ResolveTemplate for TemplatedString {
    type Output = String;

    fn resolve(&self, context: &Table) -> Result<Self::Output, TemplateError> {
        render_string(&self.0, context)
    }
}

impl ResolveTemplate for TemplatedUrl {
    type Output = String;

    fn resolve(&self, context: &Table) -> Result<Self::Output, TemplateError> {
        render_string(&self.0, context)
    }
}

pub fn render_string(template: &str, context: &Table) -> Result<String, TemplateError> {
    let mut environment = Environment::new();
    environment.set_undefined_behavior(UndefinedBehavior::Strict);
    let compiled = environment.template_from_str(template).map_err(|source| {
        TemplateError::Render {
            message: source.to_string(),
        }
    })?;

    compiled.render(context).map_err(|source| TemplateError::Render {
        message: source.to_string(),
    })
}

impl From<PathBuf> for TemplatedPath {
    fn from(path: PathBuf) -> Self {
        Self::literal(path)
    }
}

impl From<&PathBuf> for TemplatedPath {
    fn from(path: &PathBuf) -> Self {
        Self::literal(path)
    }
}

impl From<&Path> for TemplatedPath {
    fn from(path: &Path) -> Self {
        Self::literal(path)
    }
}

impl From<String> for TemplatedPath {
    fn from(path: String) -> Self {
        Self(path)
    }
}

impl From<&str> for TemplatedPath {
    fn from(path: &str) -> Self {
        Self(path.to_string())
    }
}

impl From<String> for TemplatedString {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for TemplatedString {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for TemplatedUrl {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for TemplatedUrl {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_path_resolves_to_pathbuf() {
        let context = toml::toml! {
            app = { dir = "/tmp" }
        };

        let resolved = TemplatedPath::new("{{ app.dir }}/example.txt")
            .resolve(&context)
            .unwrap();

        assert_eq!(resolved, PathBuf::from("/tmp/example.txt"));
    }

    #[test]
    fn template_string_requires_defined_variables() {
        let error = TemplatedString::new("{{ missing.value }}")
            .resolve(&Table::new())
            .unwrap_err();

        assert!(matches!(error, TemplateError::Render { .. }));
    }
}
