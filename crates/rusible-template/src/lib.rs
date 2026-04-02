use minijinja::{Environment, UndefinedBehavior};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    path::{Path, PathBuf},
    str::FromStr,
};
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Field<T> {
    #[default]
    Nil,
    Val(T),
    Tpl(String),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveValueError {
    #[error(transparent)]
    Template(#[from] TemplateError),

    #[error("failed to parse {target} from `{value}`: {message}")]
    Parse {
        target: &'static str,
        value: String,
        message: String,
    },
}

pub trait ResolveValue: Sized {
    fn resolve_value(rendered: String) -> Result<Self, ResolveValueError>;

    fn expected_type() -> &'static str {
        std::any::type_name::<Self>()
    }
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

impl<T> Field<T> {
    pub fn nil() -> Self {
        Self::Nil
    }

    pub fn val(value: T) -> Self {
        Self::Val(value)
    }

    pub fn tpl(template: impl Into<String>) -> Self {
        Self::Tpl(template.into())
    }

    pub fn is_nil(&self) -> bool {
        matches!(self, Self::Nil)
    }

    pub fn resolve(self, context: &Table) -> Result<Option<T>, ResolveValueError>
    where
        T: ResolveValue,
    {
        match self {
            Self::Nil => Ok(None),
            Self::Val(value) => Ok(Some(value)),
            Self::Tpl(template) => {
                let rendered = render_string(&template, context)?;
                T::resolve_value(rendered).map(Some)
            }
        }
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

impl ResolveValue for String {
    fn resolve_value(rendered: String) -> Result<Self, ResolveValueError> {
        Ok(rendered)
    }
}

impl ResolveValue for PathBuf {
    fn resolve_value(rendered: String) -> Result<Self, ResolveValueError> {
        Ok(PathBuf::from(rendered))
    }
}

macro_rules! impl_resolve_value_from_str {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl ResolveValue for $ty {
                fn resolve_value(rendered: String) -> Result<Self, ResolveValueError> {
                    parse_rendered::<Self>(rendered)
                }
            }
        )+
    };
}

impl_resolve_value_from_str!(bool, i32, u16, u32, u64);

pub fn render_string(template: &str, context: &Table) -> Result<String, TemplateError> {
    let mut environment = Environment::new();
    environment.set_undefined_behavior(UndefinedBehavior::Strict);
    let compiled =
        environment
            .template_from_str(template)
            .map_err(|source| TemplateError::Render {
                message: source.to_string(),
            })?;

    compiled
        .render(context)
        .map_err(|source| TemplateError::Render {
            message: source.to_string(),
        })
}

fn parse_rendered<T>(rendered: String) -> Result<T, ResolveValueError>
where
    T: FromStr,
    T::Err: fmt::Display,
    T: ResolveValue,
{
    T::from_str(&rendered).map_err(|error| ResolveValueError::Parse {
        target: T::expected_type(),
        value: rendered,
        message: error.to_string(),
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

impl<T> From<T> for Field<T> {
    fn from(value: T) -> Self {
        Self::Val(value)
    }
}

impl From<&str> for Field<String> {
    fn from(value: &str) -> Self {
        Self::Val(value.to_string())
    }
}

impl From<&Path> for Field<PathBuf> {
    fn from(value: &Path) -> Self {
        Self::Val(value.to_path_buf())
    }
}

impl From<&PathBuf> for Field<PathBuf> {
    fn from(value: &PathBuf) -> Self {
        Self::Val(value.clone())
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

    #[test]
    fn field_tpl_resolves_to_typed_value() {
        let context = toml::toml! {
            wait_for = { port = 2379 }
        };

        let resolved = Field::<u16>::tpl("{{ wait_for.port }}")
            .resolve(&context)
            .unwrap();

        assert_eq!(resolved, Some(2379));
    }

    #[test]
    fn field_nil_resolves_to_none() {
        let resolved = Field::<String>::Nil.resolve(&Table::new()).unwrap();

        assert_eq!(resolved, None);
    }
}
