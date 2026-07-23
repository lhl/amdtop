# Agent Instructions

## Development workflow

### RED/GREEN test-driven development

Use RED/GREEN TDD for every new feature and bug fix:

1. **RED:** Write or update a focused test that describes the intended behavior before changing production code. Run it and confirm that it fails for the expected reason.
2. **GREEN:** Make the smallest production-code change that satisfies the behavior, then rerun the focused test and confirm that it passes.
3. **REFACTOR:** Improve the implementation as needed while keeping the tests green.

Add regression and edge-case coverage appropriate to the change. Before considering work complete, run the full test suite plus the repository's formatting and lint checks. If hardware or terminal behavior cannot be tested directly, first extract and test the deterministic logic, then record the relevant integration or manual validation.

## Git hygiene

- Inspect `git status` before editing and preserve unrelated user or pre-existing changes.
- Keep commits focused and logically complete. Do not combine unrelated fixes, refactors, documentation, or release operations in one commit.
- Stage explicit paths, review the staged diff, and use concise imperative commit messages consistent with the project history.
- After required validation passes, commit all completed task-related changes before declaring the work finished unless the user explicitly asks to leave them uncommitted.
- Never amend, rebase, reset, force-push, discard changes, or rewrite shared history without explicit user authorization. Do not push commits unless requested.
- Do not commit secrets, credentials, build outputs, caches, or incidental generated files.
- Before reporting completion, inspect the resulting commit sequence and confirm that the working tree contains no uncommitted task-related changes.

## Releasing

Maintainers should follow the [publishing checklist](docs/PUBLISH.md), including the crates.io trusted-publishing setup and release validation gates. Use the [screenshot generation runbook](docs/SCREENSHOT.md) when updating the TUI image.
