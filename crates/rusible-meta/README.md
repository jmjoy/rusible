# rusible-meta

`rusible-meta` contains the shared serializable task model used across the
Rusible workspace.

## Responsibilities

- define task payload types
- define file, template, and command task variants
- define structured execution result types

This crate keeps the controller and executor aligned on the same wire format
without coupling either side to implementation details.

## License

Licensed under Mulan PSL v2.
