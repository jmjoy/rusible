---
name: cargo-bindeps
description: 'Run Cargo commands in this workspace with the required `-Z bindeps` flag.'
argument-hint: 'What Cargo command do you want to run in rusible?'
user-invocable: true
---

# Cargo Bindeps

## When to Use

- Running `cargo` commands in the `rusible` workspace.
- Validating code after Rust changes with `cargo check`, `cargo test`, or `cargo run`.
- Investigating build failures that may be caused by missing unstable Cargo flags.

## Outcome

Produce Cargo commands that include the required `-Z bindeps` flag so workspace commands succeed with the example crate configuration.

## Procedure

1. Confirm the command will run inside the `rusible` workspace.
2. Assume bare `cargo` commands are insufficient unless the user explicitly provides a different invocation that is already known to work.
3. Add `-Z bindeps` immediately after `cargo`.
4. Run the intended subcommand with the user's requested arguments unchanged.
5. If a Cargo command fails, check whether the failure came from omitting `-Z bindeps` before investigating deeper causes.

## Decision Rules

- Use `cargo -Z bindeps check` instead of `cargo check`.
- Use `cargo -Z bindeps test` instead of `cargo test`.
- Use `cargo -Z bindeps run` instead of `cargo run`.
- If the user already included `-Z bindeps`, do not add it again.
- If the command is not a Cargo command, this skill does not apply.

## Quality Checks

- The command starts with `cargo -Z bindeps`.
- The rest of the command preserves the user's intended subcommand and arguments.
- The command is run from the correct workspace or crate directory.

## Example Prompts

- `/cargo-bindeps Run the workspace checks`
- `/cargo-bindeps Run the hello-world example`
- `/cargo-bindeps Validate the project after editing runtime.rs`
