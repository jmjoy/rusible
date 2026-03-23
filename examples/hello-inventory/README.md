# hello-inventory

`hello-inventory` demonstrates how to load an `inventory.toml` file into
`rusible::inventory::Inventory`, select a host subset, and execute a simple
template task.

## What It Shows

- loading `inventory.toml` with `inventory::Inventory::from_toml_path`
- representing named hosts and nested groups in TOML
- filtering selected hosts by group before execution
- defining inventory-level default vars and per-host overrides in TOML
- mutating vars from Rust before execution with `set_var` and `host_mut`
- rendering templates with both custom vars and the reserved `rusible.*` namespace

## Inventory Format

The example inventory uses two top-level arrays:

- `[vars]`: default variables shared by all hosts in the inventory
- `[[groups]]`: named group definitions with optional `children`
- `[[hosts]]`: named hosts with SSH connection info, optional `vars`, and `groups`

## License

Licensed under Mulan PSL v2.
