---
name: english-doc-comments
description: 'Write code documentation comments in English and add concise explanatory comments when editing or generating Rust code. Use for Rust API docs, module docs, function docs, and non-obvious implementation notes.'
argument-hint: 'What code or file should be documented?'
user-invocable: true
---

# English Doc Comments

## When to Use

- Editing or generating Rust code that should include public API documentation.
- Adding module, struct, enum, trait, or function documentation.
- Cleaning up comments so they are concise, useful, and consistent.
- Reviewing code that lacks enough guidance for future readers.

## Outcome

Produce Rust code with:

- English doc comments for public-facing items.
- Short, useful inline comments only where the logic is not obvious.
- No redundant comments that merely restate the code.
- Comments that match the surrounding style and repository conventions.

## Procedure

1. Inspect the target file and nearby code before writing comments.
2. Determine whether the item needs documentation because it is public, reusable, externally consumed, or otherwise non-trivial.
3. Write doc comments in English.
4. Prefer Rust doc comment forms that match the target:
   - Use `///` for items.
   - Use `//!` for module-level documentation when needed.
5. Keep doc comments concrete:
   - Describe what the item does.
   - Mention key inputs, outputs, constraints, or side effects when relevant.
   - Avoid filler language and marketing phrasing.
6. Add regular comments sparingly:
   - Add a short comment before logic that is hard to infer from the code alone.
   - Explain intent, invariants, assumptions, or edge-case handling.
   - Do not comment on obvious assignments, loops, or simple conditionals.
7. Preserve existing naming, formatting, and project style.
8. Before finishing, check whether each comment still adds information. Remove comments that do not.

## Decision Rules

- If the code is public or part of the crate API, add or update doc comments unless the repository clearly avoids them in that area.
- If the code is private and straightforward, prefer no comment.
- If the code is private but subtle, add one short explanatory comment.
- If an existing comment is not in English, convert it to clear English when touching that code, unless the user asked to preserve the original language.
- If there is a conflict between this skill and established file-local style, follow the repository style and keep the comments useful.

## Quality Checks

- Doc comments are written in natural English.
- Comments explain behavior or intent, not syntax.
- Comment count is minimal but sufficient.
- Public items are easier to understand from the docs alone.
- No outdated comments remain after the edit.

## Example Prompts

- `/english-doc-comments Add docs for the public types in crates/rusible/src/task.rs`
- `/english-doc-comments Review crates/rusible/src/module.rs and add only the missing English doc comments`
- `/english-doc-comments Document this new Rust API and add brief comments for the tricky parts only`
