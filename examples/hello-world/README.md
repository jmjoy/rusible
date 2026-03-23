# hello-world

`hello-world` is a small example crate that demonstrates end-to-end Rusible task
execution.

## What it shows

- initializing `rusible-exec` as a target-side executor artifact
- connecting to a remote host through the `rusible` runtime
- setting per-remote variables from Rust before task execution
- rendering a template with both custom vars and the reserved `rusible.*` namespace

This example is intended as a minimal integration exercise for the workspace.

## License

Licensed under Mulan PSL v2.
