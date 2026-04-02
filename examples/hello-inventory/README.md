# hello-inventory

`hello-inventory` demonstrates how to load an inventory TOML file or a
directory of inventory TOML files into `rusible::inventory::Inventory`, select
a host subset, and execute a file task with templated content.

## What It Shows

- loading an inventory TOML file or directory with `inventory::Inventory::from_toml_path`
- representing named hosts and nested groups in TOML
- filtering selected hosts by group before execution
- defining inventory-level default vars and per-host overrides in TOML
- mutating vars from Rust before execution with `set_var` and `host_mut`
- rendering templates with both custom vars and the reserved `rusible.*` namespace

## Inventory Format

Inventory files accept exactly three top-level keys:

- `[vars]`: default variables shared by all hosts in the inventory
- `[[groups]]`: named group definitions with optional `children`
- `[[hosts]]`: named hosts with SSH connection info, optional `vars`, and `groups`

Rusible rejects any other top-level key so typos fail fast.

When loading a directory, Rusible recursively reads every `.toml` file,
sorts them by relative path, and merges them into one logical inventory.
`[vars]` tables merge recursively, and `[[groups]]` and `[[hosts]]` append
across files.

## License

Licensed under Mulan PSL v2.
