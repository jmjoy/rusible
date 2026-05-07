# rusible

`rusible` is the controller-side runtime for Rusible.

It prepares `rusible-exec`, builds template context, serializes tasks from
`rusible-meta`, executes them locally or over SSH, and returns structured run
reports.

## Responsibilities

- execute tasks against a local controller target or SSH-accessible remote hosts
- load and filter ansible-like inventories from TOML
- manage controller-side template variables for local, remote, and inventory scopes
- upload controller-side files before task execution when needed
- surface structured run reports and error helpers for failed or unreachable runs

Task payloads and result types come from `rusible-meta` and are re-exported as
`rusible::meta`.

## Core Types

- `runtime::Runnable`: async trait for initializing targets and running tasks
- `target::Local`: execute tasks on the controller machine
- `target::Remote`: execute tasks on one SSH target
- `inventory::Inventory`: execute tasks across named hosts and nested groups
- `report::RunResultExt`: helpers for inspecting or relaxing report-backed errors

## Runtime Flow

1. Build or embed a `rusible-exec` binary.
2. Call `init(...)` on a `Local`, `Remote`, `Vec<Remote>`, or `Inventory`.
3. Pass a concrete task such as `FileTask` to `run(...)`.
4. Await the typed report.

By default, `run(...)` returns `Err(...)` when a report contains `failed` or
`unreachable`. If you want to recover the report or change that behavior, use
`RunResultExt` helpers such as `into_report`, `ignore_unreachable`, and
`fail_on_skipped`.

For end-to-end usage examples, see `examples/hello-world` and
`examples/hello-inventory` in the workspace root.

## Inventory Support

`Inventory` can be built in code or loaded from TOML with
`Inventory::from_toml_path(...)`. Hosts carry a `Remote` target plus group
membership, and inventories can be narrowed with selectors such as
`filter_by_group(...)` and `filter_by_name(...)` before calling `run(...)`.

Inventory, remote, and local targets all expose dotted-path variable helpers
such as `set_var`, `get_var`, `merge_vars`, and `remove_var`.

## License

Licensed under Mulan PSL v2.
