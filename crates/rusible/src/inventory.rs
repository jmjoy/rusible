use crate::{
    VarError, VarLookupError,
    report::RuntimeError,
    target::{Remote, UploadOptions, UploadReport},
    vars::{get_table_path_string, merge_tables, remove_table_path, set_table_path},
};
use rusible_template::{ResolveTemplate, TemplatedPath};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};
use tokio::fs;
use toml::{Table, Value};
use tracing::info_span;

/// A named host inside an [`Inventory`].
///
/// A host wraps a [`Remote`] target and can belong to multiple groups.
#[derive(Debug, Clone)]
pub struct Host {
    pub(crate) name: String,
    pub(crate) remote: Remote,
    pub(crate) groups: Vec<String>,
}

/// A nested inventory group.
///
/// Groups define hierarchical membership such as `prod -> web`.
#[derive(Debug, Clone, Default)]
pub struct Group {
    name: String,
    groups: Vec<Group>,
}

/// An ansible-like inventory with named hosts and nested groups.
///
/// Hosts are registered separately from groups and declare the groups they
/// belong to. Filtering returns a new inventory with the same group
/// definitions and a narrowed host set.
#[derive(Debug, Clone, Default)]
pub struct Inventory {
    pub(crate) groups: Vec<Group>,
    pub(crate) hosts: Vec<Host>,
    pub(crate) vars: Table,
}

/// Result of uploading a file to one host selected by an [`Inventory`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryUploadReport {
    pub host: String,
    pub local_path: PathBuf,
    pub remote_path: String,
    pub bytes_written: usize,
}

/// Error returned when uploading a file to hosts selected by an [`Inventory`].
#[derive(Debug, thiserror::Error)]
pub enum InventoryUploadError {
    #[error("failed to upload file to host `{host}`: {source}")]
    Runtime {
        host: String,
        #[source]
        source: RuntimeError,
    },
}

impl Host {
    /// Creates a named host that points at a remote target.
    pub fn with_remote(name: impl Into<String>, remote: Remote) -> Self {
        Self {
            name: name.into(),
            remote,
            groups: Vec::new(),
        }
    }

    /// Adds a single group membership.
    pub fn add_group(mut self, group: impl Into<String>) -> Self {
        self.groups.push(group.into());
        self
    }

    /// Adds multiple group memberships.
    pub fn add_groups<I, S>(mut self, groups: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.groups.extend(groups.into_iter().map(Into::into));
        self
    }

    /// Returns the inventory host name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the remote target.
    pub fn remote(&self) -> &Remote {
        &self.remote
    }

    /// Returns the mutable remote target.
    pub fn remote_mut(&mut self) -> &mut Remote {
        &mut self.remote
    }

    /// Returns the declared group memberships.
    pub fn groups(&self) -> &[String] {
        &self.groups
    }
}

impl Group {
    /// Creates an empty group with the provided name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            groups: Vec::new(),
        }
    }

    /// Appends a direct child group.
    pub fn add_group(mut self, group: Group) -> Self {
        self.groups.push(group);
        self
    }

    /// Returns the group name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the direct child groups.
    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    pub(crate) fn collect_descendant_names(&self, names: &mut HashSet<String>) {
        if !names.insert(self.name.clone()) {
            return;
        }

        for group in &self.groups {
            group.collect_descendant_names(names);
        }
    }

    pub(crate) fn collect_group_names_for_match(&self, target: &str, names: &mut HashSet<String>) {
        if self.name == target {
            self.collect_descendant_names(names);
        }

        for group in &self.groups {
            group.collect_group_names_for_match(target, names);
        }
    }
}

impl Inventory {
    /// Creates an empty inventory.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the default template variable table for all remote hosts.
    pub fn vars(&self) -> &Table {
        &self.vars
    }

    /// Returns a string variable by dotted path.
    pub fn get_var(&self, path: impl AsRef<str>) -> Result<String, VarLookupError> {
        get_table_path_string(&self.vars, path.as_ref())
    }

    /// Returns the mutable default template variable table for all remote
    /// hosts.
    pub fn vars_mut(&mut self) -> &mut Table {
        &mut self.vars
    }

    /// Recursively merges default variables into the inventory.
    pub fn merge_vars(&mut self, vars: Table) {
        merge_tables(&mut self.vars, &vars);
    }

    /// Sets a default variable by dotted path, creating missing intermediate
    /// tables.
    pub fn set_var(
        &mut self, path: impl AsRef<str>, value: impl Into<Value>,
    ) -> Result<(), VarError> {
        set_table_path(&mut self.vars, path.as_ref(), value)
    }

    /// Removes a default variable by dotted path and returns the removed value
    /// when present.
    pub fn remove_var(&mut self, path: impl AsRef<str>) -> Result<Option<Value>, VarError> {
        remove_table_path(&mut self.vars, path.as_ref())
    }

    /// Appends a top-level group.
    pub fn add_group(mut self, group: Group) -> Self {
        self.groups.push(group);
        self
    }

    /// Appends a named host.
    pub fn add_host(mut self, host: Host) -> Self {
        self.hosts.push(host);
        self
    }

    /// Parses an inventory from TOML text.
    pub fn from_toml_str(input: &str) -> Result<Self, InventoryLoadError> {
        let parsed: InventoryToml = toml::from_str(input)?;
        parsed.try_into_inventory()
    }

    /// Parses an inventory from a TOML file on disk.
    pub async fn from_toml_path<P>(path: P) -> Result<Self, InventoryLoadError>
    where
        P: AsRef<Path>,
    {
        let path = path.as_ref();
        let input = fs::read_to_string(path)
            .await
            .map_err(|source| InventoryLoadError::Io {
                path: path.to_path_buf(),
                source,
            })?;

        Self::from_toml_str(&input)
    }

    /// Returns a new `Inventory` containing only hosts reachable through the
    /// matched group and its descendant groups.
    pub fn filter_by_group(&self, group: &str) -> Self {
        let mut group_names = HashSet::new();
        for root in &self.groups {
            root.collect_group_names_for_match(group, &mut group_names);
        }

        Self {
            groups: self.groups.clone(),
            hosts: self
                .hosts
                .iter()
                .filter(|host| host.groups.iter().any(|name| group_names.contains(name)))
                .cloned()
                .collect(),
            vars: self.vars.clone(),
        }
    }

    /// Returns a new `Inventory` containing only the named host.
    pub fn filter_by_name(&self, name: &str) -> Self {
        self.filter_by_names([name])
    }

    /// Returns a new `Inventory` containing only hosts whose names are in the
    /// provided set.
    pub fn filter_by_names<I, S>(&self, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let names = names.into_iter().map(Into::into).collect::<HashSet<_>>();

        Self {
            groups: self.groups.clone(),
            hosts: self
                .hosts
                .iter()
                .filter(|host| names.contains(host.name()))
                .cloned()
                .collect(),
            vars: self.vars.clone(),
        }
    }

    /// Returns all top-level groups.
    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    /// Returns all hosts in their declared order.
    pub fn hosts(&self) -> &[Host] {
        &self.hosts
    }

    /// Uploads a local controller file to selected hosts, rendering per-host
    /// template paths when requested.
    pub async fn upload_file(
        &self, local_path: impl Into<TemplatedPath>, remote_path: impl Into<TemplatedPath>,
        options: UploadOptions,
    ) -> Result<Vec<InventoryUploadReport>, InventoryUploadError> {
        let local_path = local_path.into();
        let remote_path = remote_path.into();
        let upload_span = info_span!("UPLOAD", host_count = self.hosts.len());
        let _upload_guard = upload_span.enter();
        let mut uploads = Vec::with_capacity(self.hosts.len());

        for host in &self.hosts {
            let host_span = info_span!(parent: &upload_span, "HOST", host = %host.name());
            let _host_guard = host_span.enter();
            let context = host
                .remote()
                .build_context(Some(&self.vars), Some(host.name()));
            let rendered_local_path = local_path.resolve(&context).map_err(|source| {
                InventoryUploadError::Runtime {
                    host: host.name().to_string(),
                    source: RuntimeError::from(source),
                }
            })?;
            let UploadReport {
                remote_path,
                bytes_written,
            } = host
                .remote()
                .upload_file(
                    rendered_local_path.clone(),
                    remote_path.resolve(&context).map_err(|source| {
                        InventoryUploadError::Runtime {
                            host: host.name().to_string(),
                            source: RuntimeError::from(source),
                        }
                    })?,
                    options.clone(),
                )
                .await
                .map_err(|source| InventoryUploadError::Runtime {
                    host: host.name().to_string(),
                    source,
                })?;

            uploads.push(InventoryUploadReport {
                host: host.name().to_string(),
                local_path: rendered_local_path,
                remote_path,
                bytes_written,
            });
        }

        Ok(uploads)
    }

    /// Returns a mutable host by inventory name.
    pub fn host_mut(&mut self, name: &str) -> Option<&mut Host> {
        self.hosts.iter_mut().find(|host| host.name() == name)
    }

    /// Returns `true` if no hosts are selected.
    pub fn is_empty(&self) -> bool {
        self.hosts.is_empty()
    }

    /// Returns the total number of selected hosts.
    pub fn len(&self) -> usize {
        self.hosts.len()
    }
}

/// Error returned while loading an [`Inventory`] from TOML.
#[derive(Debug, thiserror::Error)]
pub enum InventoryLoadError {
    #[error("failed to read inventory file {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid inventory TOML: {source}")]
    Toml {
        #[from]
        source: toml::de::Error,
    },

    #[error("duplicate host name `{name}` in inventory")]
    DuplicateHostName { name: String },

    #[error("duplicate group name `{name}` in inventory")]
    DuplicateGroupName { name: String },

    #[error("host `{host}` references unknown group `{group}`")]
    UnknownHostGroup { host: String, group: String },

    #[error("group `{group}` references unknown child group `{child}`")]
    UnknownChildGroup { group: String, child: String },

    #[error("group nesting contains a cycle involving `{group}`")]
    GroupCycle { group: String },
}

#[derive(Debug, Deserialize, Default)]
struct InventoryToml {
    #[serde(default)]
    vars: Table,
    #[serde(default)]
    groups: Vec<InventoryTomlGroup>,
    #[serde(default)]
    hosts: Vec<InventoryTomlHost>,
}

#[derive(Debug, Deserialize)]
struct InventoryTomlGroup {
    name: String,
    #[serde(default)]
    children: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct InventoryTomlHost {
    name: String,
    host: String,
    #[serde(default = "default_remote_port")]
    port: u16,
    user: String,
    password: Option<String>,
    key: Option<PathBuf>,
    #[serde(default)]
    vars: Table,
    #[serde(default)]
    groups: Vec<String>,
}

impl InventoryToml {
    fn try_into_inventory(self) -> Result<Inventory, InventoryLoadError> {
        let mut group_children_by_name = HashMap::new();
        let mut referenced_children = HashSet::new();
        let mut group_names_in_order = Vec::new();

        for group in self.groups {
            if group_children_by_name
                .insert(group.name.clone(), group.children)
                .is_some()
            {
                return Err(InventoryLoadError::DuplicateGroupName { name: group.name });
            }
            group_names_in_order.push(group.name);
        }

        for (group, children) in &group_children_by_name {
            for child in children {
                if !group_children_by_name.contains_key(child) {
                    return Err(InventoryLoadError::UnknownChildGroup {
                        group: group.clone(),
                        child: child.clone(),
                    });
                }
                referenced_children.insert(child.clone());
            }
        }

        let mut built_groups = HashMap::new();
        let mut active_stack = HashSet::new();
        let root_groups = group_names_in_order
            .iter()
            .filter(|name| !referenced_children.contains(*name))
            .map(|name| {
                build_group_tree(
                    name,
                    &group_children_by_name,
                    &mut built_groups,
                    &mut active_stack,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        for name in &group_names_in_order {
            if !built_groups.contains_key(name) {
                build_group_tree(
                    name,
                    &group_children_by_name,
                    &mut built_groups,
                    &mut active_stack,
                )?;
            }
        }

        let mut seen_host_names = HashSet::new();
        let mut hosts = Vec::with_capacity(self.hosts.len());
        for host in self.hosts {
            if !seen_host_names.insert(host.name.clone()) {
                return Err(InventoryLoadError::DuplicateHostName { name: host.name });
            }

            for group in &host.groups {
                if !group_children_by_name.contains_key(group) {
                    return Err(InventoryLoadError::UnknownHostGroup {
                        host: host.name.clone(),
                        group: group.clone(),
                    });
                }
            }

            let mut remote = Remote::new(host.host, host.port, host.user, host.password, host.key);
            remote.merge_vars(host.vars);

            hosts.push(Host::with_remote(host.name, remote).add_groups(host.groups));
        }

        Ok(Inventory {
            groups: root_groups,
            hosts,
            vars: self.vars,
        })
    }
}

fn build_group_tree(
    name: &str, children_by_name: &HashMap<String, Vec<String>>,
    built_groups: &mut HashMap<String, Group>, active_stack: &mut HashSet<String>,
) -> Result<Group, InventoryLoadError> {
    if let Some(group) = built_groups.get(name) {
        return Ok(group.clone());
    }

    if !active_stack.insert(name.to_string()) {
        return Err(InventoryLoadError::GroupCycle {
            group: name.to_string(),
        });
    }

    let mut group = Group::new(name.to_string());
    for child in children_by_name.get(name).into_iter().flatten() {
        group = group.add_group(build_group_tree(
            child,
            children_by_name,
            built_groups,
            active_stack,
        )?);
    }

    active_stack.remove(name);
    built_groups.insert(name.to_string(), group.clone());
    Ok(group)
}

fn default_remote_port() -> u16 {
    22
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_remote(host: &str) -> Remote {
        Remote::new(host, 22, "user", None, None)
    }

    fn make_inventory() -> Inventory {
        let mut inventory = Inventory::new()
            .add_group(
                Group::new("prod")
                    .add_group(Group::new("web"))
                    .add_group(Group::new("db")),
            )
            .add_group(Group::new("ops").add_group(Group::new("monitoring")))
            .add_host(Host::with_remote("web-1", make_remote("10.0.0.11")).add_group("web"))
            .add_host(Host::with_remote("db-1", make_remote("10.0.0.21")).add_group("db"))
            .add_host(
                Host::with_remote("bastion-1", make_remote("10.0.0.31"))
                    .add_groups(["web", "monitoring"]),
            );
        inventory.set_var("region", "cn-north-1").unwrap();
        inventory
    }

    const INVENTORY_TOML: &str = r#"
[vars]
region = "cn-north-1"

[vars.app]
name = "rusible"

[[groups]]
name = "prod"
children = ["web", "db"]

[[groups]]
name = "ops"
children = ["monitoring"]

[[groups]]
name = "web"

[[groups]]
name = "db"

[[groups]]
name = "monitoring"

[[hosts]]
name = "web-1"
host = "10.0.0.11"
port = 2222
user = "root"
password = "secret"
vars = { role = "web" }
groups = ["web"]

[[hosts]]
name = "db-1"
host = "10.0.0.21"
user = "root"
key = "/tmp/db.pem"
vars = { role = "db" }
groups = ["db"]

[[hosts]]
name = "bastion-1"
host = "10.0.0.31"
port = 2224
user = "root"
password = "secret"
groups = ["web", "monitoring"]
"#;

    #[test]
    fn inventory_filter_by_group_includes_descendant_groups() {
        let inventory = make_inventory();

        let prod = inventory.filter_by_group("prod");
        assert_eq!(prod.len(), 3);
        assert_eq!(prod.hosts()[0].name(), "web-1");
        assert_eq!(prod.hosts()[1].name(), "db-1");
        assert_eq!(prod.hosts()[2].name(), "bastion-1");
    }

    #[test]
    fn inventory_filter_by_leaf_group_returns_matching_hosts() {
        let inventory = make_inventory();

        let web = inventory.filter_by_group("web");
        assert_eq!(web.len(), 2);
        assert_eq!(web.hosts()[0].name(), "web-1");
        assert_eq!(web.hosts()[1].name(), "bastion-1");
    }

    #[test]
    fn inventory_filter_by_name_returns_single_host() {
        let inventory = make_inventory();

        let host = inventory.filter_by_name("db-1");
        assert_eq!(host.len(), 1);
        assert_eq!(host.hosts()[0].name(), "db-1");
    }

    #[test]
    fn inventory_filter_by_names_returns_multiple_hosts() {
        let inventory = make_inventory();

        let selected = inventory.filter_by_names(["web-1", "bastion-1"]);
        assert_eq!(selected.len(), 2);
        assert_eq!(selected.hosts()[0].name(), "web-1");
        assert_eq!(selected.hosts()[1].name(), "bastion-1");
    }

    #[test]
    fn inventory_chained_filters_intersect_host_sets() {
        let inventory = make_inventory();

        let selected = inventory.filter_by_group("ops").filter_by_name("bastion-1");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected.hosts()[0].name(), "bastion-1");
    }

    #[test]
    fn inventory_len_counts_hosts() {
        let inventory = make_inventory();

        assert_eq!(inventory.len(), 3);
        assert!(!inventory.is_empty());
    }

    #[test]
    fn inventory_filter_returns_empty_when_no_match() {
        let inventory = make_inventory();

        assert!(inventory.filter_by_group("missing").is_empty());
        assert!(inventory.filter_by_name("missing").is_empty());
    }

    #[test]
    fn inventory_parses_from_toml() {
        let inventory = Inventory::from_toml_str(INVENTORY_TOML).unwrap();

        assert_eq!(inventory.groups().len(), 2);
        assert_eq!(inventory.groups()[0].name(), "prod");
        assert_eq!(inventory.groups()[0].groups()[0].name(), "web");
        assert_eq!(inventory.hosts().len(), 3);
        assert_eq!(inventory.hosts()[0].name(), "web-1");
        assert_eq!(inventory.hosts()[0].remote().port, 2222);
        assert_eq!(inventory.hosts()[1].remote().port, 22);
        assert_eq!(inventory.vars()["region"].as_str(), Some("cn-north-1"));
        assert_eq!(
            inventory.hosts()[0].remote().vars["role"].as_str(),
            Some("web")
        );
    }

    #[test]
    fn inventory_var_mutation_apis_work() {
        let mut inventory = make_inventory();

        inventory.set_var("app.name", "rusible").unwrap();
        inventory
            .host_mut("web-1")
            .unwrap()
            .remote_mut()
            .set_var("app.port", 8080)
            .unwrap();

        assert_eq!(inventory.vars()["app"]["name"].as_str(), Some("rusible"));
        assert_eq!(
            inventory.hosts()[0].remote().vars["app"]["port"].as_integer(),
            Some(8080)
        );
    }

    #[test]
    fn inventory_get_var_reads_default_strings() {
        let inventory = make_inventory();

        assert_eq!(inventory.get_var("region").unwrap(), "cn-north-1");
    }

    #[test]
    fn inventory_parse_rejects_unknown_host_group() {
        let inventory_toml = r#"
[[groups]]
name = "web"

[[hosts]]
name = "web-1"
host = "127.0.0.1"
user = "root"
groups = ["missing"]
"#;

        assert!(matches!(
            Inventory::from_toml_str(inventory_toml),
            Err(InventoryLoadError::UnknownHostGroup { .. })
        ));
    }

    #[test]
    fn inventory_parse_rejects_group_cycles() {
        let inventory_toml = r#"
[[groups]]
name = "prod"
children = ["web"]

[[groups]]
name = "web"
children = ["prod"]
"#;

        assert!(matches!(
            Inventory::from_toml_str(inventory_toml),
            Err(InventoryLoadError::GroupCycle { .. })
        ));
    }
}
