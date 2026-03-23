# rusible-exec

`rusible-exec` is the target-side executor for Rusible.

It reads a serialized task from standard input, applies the requested file or
template operation on the local machine, can also execute shell-free commands,
and prints a structured task result as JSON.

## Responsibilities

- deserialize tasks from `rusible-meta`
- apply file, template, and command operations
- report changed, ok, failed, or unreachable status back to the controller

This crate is intended to be launched by the `rusible` runtime rather than used
as a standalone CLI for manual task authoring.

## License

Licensed under Mulan PSL v2.
