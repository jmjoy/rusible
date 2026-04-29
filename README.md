# rusible

Rusible is a Rust workspace for infrastructure automation.

The workspace is currently split into three crates:

- `rusible`: controller-side runtime for executing tasks locally or over SSH
- `rusible-exec`: target-side executor binary that applies serialized tasks
- `rusible-meta`: shared serializable task definitions used by both sides

Currently supported tasks include `file`, `command`, `copy`, `download`,
`facts`, `shell`, `stat`, `user`, `systemd`, `unarchive`, and `wait_for`.

## Workspace layout

- `crates/rusible`: main library crate
- `crates/rusible-exec`: executor binary crate
- `crates/rusible-meta`: shared metadata crate
- `examples/hello-world`: end-to-end example using the workspace
- `examples/hello-inventory`: inventory-driven example
- `examples/install-etcd`: etcd installation example with local certificate generation

## Development

This workspace uses the nightly toolchain and enables `bindeps` through
`.cargo/config.toml`, so Cargo commands work from the repository root without
passing `-Z bindeps` explicitly.

```bash
cargo check
```

## Runtime behavior

`Runnable::run` now returns an error when a task result contains `failed` or
`unreachable`.

If you want to recover the report or relax that default, import
`RunResultExt` and use helpers such as `into_report`, `ignore_unreachable`, and
`fail_on_skipped`.

## License

Licensed under Mulan PSL v2.
