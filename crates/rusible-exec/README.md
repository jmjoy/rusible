# rusible-exec

`rusible-exec` is the target-side executor for Rusible.

It reads a serialized task from standard input, applies file, copy, template,
or unarchive operations on the local machine, can execute shell-free or
shell-backed commands, and can also perform basic host-management tasks such as
stat, user, systemd, and wait-for checks before printing a structured task
result as JSON.

## Responsibilities

- deserialize tasks from `rusible-meta`
- apply file, template, copy, unarchive, command, shell, stat, user, systemd, and wait-for operations
- report changed, ok, failed, or unreachable status back to the controller

This crate is intended to be launched by the `rusible` runtime rather than used
as a standalone CLI for manual task authoring.

## License

Licensed under Mulan PSL v2.
