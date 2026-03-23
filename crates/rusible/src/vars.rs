use toml::{Table, Value};

/// Error returned when manipulating controller-side template variables.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VarError {
    #[error("variable path cannot be empty")]
    EmptyPath,

    #[error("variable path `{path}` contains an empty segment")]
    EmptySegment { path: String },

    #[error("variable path `{path}` cannot descend through non-table segment `{segment}`")]
    PathConflict { path: String, segment: String },
}

pub(crate) fn merge_tables(base: &mut Table, overlay: &Table) {
    for (key, overlay_value) in overlay {
        match (base.get_mut(key), overlay_value) {
            (Some(Value::Table(base_table)), Value::Table(overlay_table)) => {
                merge_tables(base_table, overlay_table);
            }
            _ => {
                base.insert(key.clone(), overlay_value.clone());
            }
        }
    }
}

pub(crate) fn set_table_path(
    table: &mut Table, path: &str, value: impl Into<Value>,
) -> Result<(), VarError> {
    let segments = parse_path(path)?;
    let mut current = table;

    for segment in &segments[..segments.len() - 1] {
        let entry = current
            .entry((*segment).to_string())
            .or_insert_with(|| Value::Table(Table::new()));
        match entry {
            Value::Table(next) => current = next,
            _ => {
                return Err(VarError::PathConflict {
                    path: path.to_string(),
                    segment: (*segment).to_string(),
                });
            }
        }
    }

    current.insert(
        segments
            .last()
            .expect("path has at least one segment")
            .to_string(),
        value.into(),
    );
    Ok(())
}

pub(crate) fn remove_table_path(table: &mut Table, path: &str) -> Result<Option<Value>, VarError> {
    let segments = parse_path(path)?;
    let mut current = table;

    for segment in &segments[..segments.len() - 1] {
        let Some(next) = current.get_mut(*segment) else {
            return Ok(None);
        };

        match next {
            Value::Table(next) => current = next,
            _ => {
                return Err(VarError::PathConflict {
                    path: path.to_string(),
                    segment: (*segment).to_string(),
                });
            }
        }
    }

    Ok(current.remove(*segments.last().expect("path has at least one segment")))
}

pub(crate) fn build_local_context(vars: &Table) -> Table {
    let mut context = vars.clone();
    context.insert("rusible".to_string(), Value::Table(local_namespace()));
    context
}

pub(crate) fn build_remote_context(
    defaults: Option<&Table>, remote_vars: &Table, host_name: Option<&str>, host: &str, port: u16,
    user: &str,
) -> Table {
    let mut context = defaults.cloned().unwrap_or_default();
    merge_tables(&mut context, remote_vars);
    context.insert(
        "rusible".to_string(),
        Value::Table(remote_namespace(host_name, host, port, user)),
    );
    context
}

fn parse_path(path: &str) -> Result<Vec<&str>, VarError> {
    if path.is_empty() {
        return Err(VarError::EmptyPath);
    }

    let segments = path.split('.').collect::<Vec<_>>();
    if segments.iter().any(|segment| segment.is_empty()) {
        return Err(VarError::EmptySegment {
            path: path.to_string(),
        });
    }

    Ok(segments)
}

fn local_namespace() -> Table {
    let mut target = Table::new();
    target.insert("kind".to_string(), Value::String("local".to_string()));

    let mut rusible = Table::new();
    rusible.insert("target".to_string(), Value::Table(target));
    rusible
}

fn remote_namespace(host_name: Option<&str>, host: &str, port: u16, user: &str) -> Table {
    let mut target = Table::new();
    target.insert("kind".to_string(), Value::String("remote".to_string()));

    let mut host_table = Table::new();
    host_table.insert("host".to_string(), Value::String(host.to_string()));
    host_table.insert("port".to_string(), Value::Integer(i64::from(port)));
    host_table.insert("user".to_string(), Value::String(user.to_string()));
    if let Some(host_name) = host_name {
        host_table.insert("name".to_string(), Value::String(host_name.to_string()));
    }

    let mut rusible = Table::new();
    rusible.insert("target".to_string(), Value::Table(target));
    rusible.insert("host".to_string(), Value::Table(host_table));
    rusible
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_tables_recursively_overrides_leaf_values() {
        let mut base = Table::new();
        set_table_path(&mut base, "app.name", "rusible").unwrap();
        set_table_path(&mut base, "app.port", 80).unwrap();

        let mut overlay = Table::new();
        set_table_path(&mut overlay, "app.port", 8080).unwrap();
        set_table_path(&mut overlay, "app.mode", "prod").unwrap();

        merge_tables(&mut base, &overlay);

        assert_eq!(base["app"]["name"].as_str(), Some("rusible"));
        assert_eq!(base["app"]["port"].as_integer(), Some(8080));
        assert_eq!(base["app"]["mode"].as_str(), Some("prod"));
    }

    #[test]
    fn set_and_remove_table_paths() {
        let mut table = Table::new();
        set_table_path(&mut table, "app.name", "rusible").unwrap();
        assert_eq!(table["app"]["name"].as_str(), Some("rusible"));

        let removed = remove_table_path(&mut table, "app.name").unwrap();
        assert_eq!(removed.unwrap().as_str(), Some("rusible"));
    }

    #[test]
    fn path_conflicts_when_descending_through_scalar() {
        let mut table = Table::new();
        set_table_path(&mut table, "app", "rusible").unwrap();

        assert!(matches!(
            set_table_path(&mut table, "app.name", "value"),
            Err(VarError::PathConflict { .. })
        ));
    }

    #[test]
    fn remote_context_overrides_reserved_rusible_namespace() {
        let mut defaults = Table::new();
        set_table_path(&mut defaults, "rusible.target.kind", "broken").unwrap();
        let mut remote = Table::new();
        set_table_path(&mut remote, "app.name", "rusible").unwrap();

        let context = build_remote_context(
            Some(&defaults),
            &remote,
            Some("web-1"),
            "10.0.0.11",
            22,
            "root",
        );

        assert_eq!(context["app"]["name"].as_str(), Some("rusible"));
        assert_eq!(
            context["rusible"]["target"]["kind"].as_str(),
            Some("remote")
        );
        assert_eq!(context["rusible"]["host"]["name"].as_str(), Some("web-1"));
    }
}
