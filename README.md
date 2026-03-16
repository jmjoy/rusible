# rusible

Rusible is a Rust workspace for infrastructure automation.

The workspace is currently split into three crates:

- `rusible`: controller-side runtime for executing tasks locally or over SSH
- `rusible-exec`: target-side executor binary that applies serialized tasks
- `rusible-meta`: shared serializable task definitions used by both sides

## Workspace layout

- `crates/rusible`: main library crate
- `crates/rusible-exec`: executor binary crate
- `crates/rusible-meta`: shared metadata crate
- `examples/hello-world`: end-to-end example using the workspace

## Development

This workspace expects Cargo commands to run with `-Z bindeps`.

```bash
cargo -Z bindeps check
```

## License

Licensed under Mulan PSL v2.
