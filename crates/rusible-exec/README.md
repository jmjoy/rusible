# rusible-exec

`rusible-exec` is the target-side executor for Rusible.

It reads a serialized task from standard input, executes `file`, `command`,
`copy`, `download`, `facts`, `shell`, `stat`, `user`, `systemd`,
`unarchive`, and `wait_for` tasks on the local machine, and prints a
structured task result as JSON.

## Responsibilities

- deserialize tasks from `rusible-meta`
- apply `file`, `command`, `copy`, `download`, `facts`, `shell`, `stat`, `user`, `systemd`, `unarchive`, and `wait_for` tasks
- report changed, ok, failed, or unreachable status back to the controller

This crate is intended to be launched by the `rusible` runtime rather than used
as a standalone CLI for manual task authoring.

## License

Licensed under Mulan PSL v2.
