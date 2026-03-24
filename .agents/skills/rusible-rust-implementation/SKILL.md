---
name: rusible-rust-implementation
description: 'Implement Rust and Cargo changes in the rusible workspace. Use when editing Rust crates, refactoring internals, or adding dependencies. Prefer the cleanest design over compatibility shims because the project is still under development. Keep dependency versions in workspace.dependencies and add crate-specific features in each crate as needed.'
argument-hint: 'What Rust or Cargo change should be implemented in rusible?'
user-invocable: true
---

# Rusible Rust Implementation

## When to Use

- Implementing or refactoring Rust code in this workspace.
- Changing public or internal APIs while the project is still in development.
- Adding, removing, or updating Cargo dependencies.
- Editing crate `Cargo.toml` files.

## Outcome

Produce Rust changes that favor a clean design over compatibility scaffolding, and keep dependency management aligned with the workspace Cargo conventions.

In this workspace, development-stage cleanup is preferred over compatibility preservation, and dependency versions are centralized even when only one crate currently uses the dependency.

## Procedure

1. Confirm the task affects Rust source code, crate manifests, or workspace dependency structure.
2. If a design tradeoff appears between backward compatibility and a cleaner implementation, prefer the cleaner implementation unless the user explicitly asks to preserve compatibility.
3. Remove transitional code, legacy branches, and compatibility wrappers instead of preserving them by default.
4. When adding or updating any dependency version, declare that version in the root `Cargo.toml` under `[workspace.dependencies]`, even if only one crate currently uses it.
5. In crate manifests, reference dependencies through `{ workspace = true }` rather than repeating versions locally.
6. If a crate needs specific features or `default-features = false`, configure those options in that crate's dependency entry while still sourcing the version from the workspace.
7. If a cleaner API or data model breaks examples, tests, or other crate call sites, update those call sites as part of the same change instead of preserving the old behavior.
8. Keep changes minimal but structural: solve the root design issue rather than layering exceptions around it.
9. Before finishing, check that the dependency layout still reads consistently across the workspace and that affected call sites were updated coherently.

## Decision Rules

- Do not add compatibility shims, deprecated aliases, fallback APIs, or migration-only branches unless the user explicitly requests them.
- If a cleaner API requires changing call sites, update the call sites instead of preserving the old API.
- If the change breaks examples, tests, or sibling crates, update them in the same change by default.
- Put shared dependency versions in `[workspace.dependencies]` at the workspace root.
- Put single-crate dependency versions in `[workspace.dependencies]` at the workspace root as well.
- In crate `Cargo.toml` files, prefer entries such as `tokio = { workspace = true, features = ["rt"] }` over restating the version.
- Features are crate-local decisions and may differ between crates.
- If a dependency is only used by one crate, it should still use the workspace version table.
- Do not duplicate version numbers across crate manifests.

## Quality Checks

- The implementation is the simplest clean design for the current codebase state.
- No unnecessary compatibility layer was introduced.
- New or changed dependency versions are defined in the root `[workspace.dependencies]` table.
- Crate manifests use `{ workspace = true }` for shared dependency versions.
- Any crate-specific feature flags are declared only where that crate needs them.
- Affected examples, tests, and sibling crates were updated when the design change required it.

## Example Prompts

- `/rusible-rust-implementation Refactor the runtime API without preserving the old shape`
- `/rusible-rust-implementation Add a new dependency and wire it through the workspace manifests`
- `/rusible-rust-implementation Clean up this module and remove compatibility code`
- `/rusible-rust-implementation Change this API and update all broken call sites in the workspace`
