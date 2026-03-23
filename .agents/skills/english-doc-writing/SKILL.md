---
name: english-doc-writing
description: 'Write repository-facing documentation in English. Use for README files, Markdown docs, Rust comments, and Rust doc comments in this workspace. Do not apply to chat replies unless the user explicitly requests English.'
argument-hint: 'What document or comment change should be written in English?'
user-invocable: true
---

# English Documentation Writing

## When to Use

- Creating or updating README files in this workspace.
- Writing Markdown documentation for examples, crates, or repository guides.
- Adding or editing Rust comments such as `//` and `/* */` comments.
- Adding or editing Rust doc comments such as `///` and `//!` comments.

## Outcome

Produce repository-facing documentation in clear English so shared project materials remain consistent for contributors and users.

This skill does not force the language of chat replies, planning notes, or other non-repository communication.

## Procedure

1. Identify whether the requested change affects repository-facing text.
2. If the text is a README, Markdown document, Rust comment, or Rust doc comment, write or rewrite it in English.
3. Preserve technical meaning, API names, command names, and code snippets exactly unless the task also requires changing them.
4. Prefer concise engineering language over marketing language.
5. If existing nearby documentation is not in English and the current task touches it, normalize the edited portion to English.
6. Before finishing, read the changed text once for grammar, clarity, and terminology consistency.

## Decision Rules

- Apply this skill to documentation stored in Markdown files such as `README.md` and crate guides.
- Apply this skill to inline Rust comments and documentation comments.
- Do not apply this skill to chat responses, clarifying questions, or progress updates unless the user explicitly asks for English there as well.
- Do not apply this skill to terminal explanations, implementation summaries, or other conversational text that is not being written into repository files.
- Do not translate source code identifiers, CLI flags, environment variables, file paths, or literal output.
- Do not rewrite unrelated files that are outside the current task.
- If the user explicitly requests another language for a specific document, follow the user's request for that document.

## Quality Checks

- New or updated documentation text is written in English.
- Rust comments and Rust doc comments introduced by the change are written in English.
- Terminology matches the surrounding crate or feature names.
- Sentences are direct and technically precise.

## Example Prompts

- `/english-doc-writing Update the root README for the new runtime behavior`
- `/english-doc-writing Add English doc comments to the inventory parser`
- `/english-doc-writing Rewrite the example README and inline Rust comments in English`
