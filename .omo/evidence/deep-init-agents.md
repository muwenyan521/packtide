# Deep Init AGENTS Evidence

Date: 2026-10-06
Base: `557b254` on `master`

## Discovery

- `find . -name AGENTS.md` found no repository-local instructions before this change.
- `Cargo.toml` defines one Rust 2024 workspace with four members: `system-tools-core`, `packtide`, `systide`, and `package-manager-matrix`.
- `cargo metadata --no-deps --format-version 1` confirmed one library target, two application binaries, one matrix binary, and the integration-test target names documented in the generated files.
- Repository code is concentrated in `crates/packtide`, `crates/system-tools-core`, `crates/systide`, and `tools/package-manager-matrix`; the backend, picker UI, mirror transaction, and fixture directories have distinct contracts.
- `rust-analyzer 1 (9074e9b4c6 2026-09-06)` is installed. The symbol inventory was collected from Rust source declarations; no LSP edits or source changes were required.
- `sg`/`ast-grep` was not installed, so no AST rewrite or AST-derived map was used.

## Generated files

```text
AGENTS.md
crates/packtide/AGENTS.md
crates/packtide/src/ui/AGENTS.md
crates/packtide/src/mirror_update/AGENTS.md
crates/system-tools-core/AGENTS.md
crates/system-tools-core/src/backends/AGENTS.md
crates/systide/AGENTS.md
tools/package-manager-matrix/AGENTS.md
tests/package-managers/AGENTS.md
```

The root file records workspace structure, code map, commands, project-specific conventions, and forbidden behavior. Child files are limited to their module boundary and avoid repeating parent guidance. No generated AGENTS file contains `.omo`, an absolute machine path, credentials, or business-code edits.

## Verification

Commands run:

```bash
find . -name AGENTS.md -not -path './.git/*' -print -exec wc -l {} \;
rg -n '\.omo|/home/|/tmp/|file:' --glob 'AGENTS.md' .
git diff --check
```

Observed results:

- 9 generated files, 27-77 lines each; root is within the init-deep 50-150 line gate and children are within the 30-80 line guidance except intentionally concise 27-29 line leaf boundaries.
- The forbidden-content search returned no matches.
- `git diff --check` returned success.
- The worktree already contained unrelated changes from other agents. Only the nine AGENTS paths and this evidence file belong to this task; no unrelated path was staged.

## Commands documented for later use

The generated root and package files document the verified workspace commands: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo build --workspace --release`, focused package tests, and the matrix tool's `doctor`, `list-images`, and `audit-cleanup`. Full build/test execution is outside this documentation-only change.
