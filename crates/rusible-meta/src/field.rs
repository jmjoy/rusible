use minijinja::{Environment, UndefinedBehavior};
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

impl ResolveValue for url::Url {
    fn resolve_value(rendered: String) -> Result<Self, ResolveValueError> {
        parse_rendered::<Self>(rendered)
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn field_tpl_requires_defined_variables() {
        let error = Field::<String>::tpl("{{ missing.value }}")
            .resolve(&Table::new())
            .unwrap_err();

        assert!(matches!(
            error,
            ResolveValueError::Template(TemplateError::Render { .. })
        ));
    }
}
