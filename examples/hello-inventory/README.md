# hello-inventory

`hello-inventory` demonstrates how to load an `inventory.toml` file into
`rusible::Inventory`, select a host subset, and execute a simple template task.

## What It Shows

- loading `inventory.toml` with `Inventory::from_toml_path`
- representing named hosts and nested groups in TOML
- filtering selected hosts by group before execution
- running a simple `TemplateTask` through the loaded inventory

## Inventory Format

The example inventory uses two top-level arrays:

- `[[groups]]`: named group definitions with optional `children`
- `[[hosts]]`: named hosts with SSH connection info and `groups`

## License

Licensed under Mulan PSL v2.
