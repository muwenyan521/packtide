# PROJECT KNOWLEDGE BASE

**Generated:** 2026-10-06
**Base commit:** `557b254`
**Branch:** `master`

## OVERVIEW
This is a Rust 2024 workspace for independent Linux package and system maintenance tools. `packtide` owns the package picker/CLI, `systide` owns system-update orchestration, and `system-tools-core` is the typed backend, command-plan, and privilege boundary shared by both.

## STRUCTURE
```text
crates/
├── system-tools-core/  # backend identities, parsers, plans, command execution
├── packtide/           # package install/remove/update TUI and CLI
└── systide/            # native and optional system update flow
tools/
└── package-manager-matrix/ # disposable container/VM integration runner
tests/
└── package-managers/   # locked image metadata and command-output fixtures
docs/                   # backend support contract
packaging/              # Arch package recipe
commands/               # release checksum record
```

## WHERE TO LOOK
| Task | Location | Notes |
|---|---|---|
| Add or change a package-manager identity/capability | `crates/system-tools-core/src/backend.rs` | Keep `BackendId`, scope, kind, capabilities, and typed errors aligned. |
| Change backend parsing or argv | `crates/system-tools-core/src/backends/` | Each provider owns parsing and `CommandPlan` construction. |
| Change privilege, locale, or subprocess behavior | `crates/system-tools-core/src/command.rs`, `plan.rs`, `transaction.rs` | System writes go through the typed privilege boundary. |
| Change package picker, preview, or hidden row identity | `crates/packtide/src/ui/`, `sources.rs`, `source_metadata.rs` | Display labels must not replace the hidden typed identity. |
| Change system update ordering or optional-provider diagnostics | `crates/systide/src/update.rs`, `operations.rs` | Native update runs before optional providers; failures remain observable. |
| Change matrix probes or cleanup | `tools/package-manager-matrix/src/main.rs` | Use locked image metadata and preserve interruption cleanup. |
| Change fixture shape | `tests/package-managers/` | Keep fixture output compatible with the parser and lock-file contracts. |

## CODE MAP
| Symbol | Location | Role |
|---|---|---|
| `BackendId`, `PackageIdentity`, `ReadOperation`, `WriteOperation` | `crates/system-tools-core/src/backend.rs` | Typed package/backend contract. |
| `BackendRegistry` | `crates/system-tools-core/src/registry.rs` | Routes typed operations to built-in backends. |
| `CommandPlan`, `CommandPrivilege` | `crates/system-tools-core/src/plan.rs`, `transaction.rs` | Explicit argv, locale, environment, and privilege. |
| `packtide::app::run` | `crates/packtide/src/app.rs` | CLI dispatch and native backend entry. |
| `systide::main` | `crates/systide/src/main.rs` | CLI parsing, confirmation, and update orchestration. |
| `package-manager-matrix::main` | `tools/package-manager-matrix/src/main.rs` | Doctor, locked-image probes, VM lane, and cleanup commands. |

## CONVENTIONS
- Workspace edition is Rust 2024; dependencies and package versions are centralized in the workspace manifest where possible.
- User-visible text lives in each binary's `locales/en.txt` and `locales/zh.txt`; do not put new UI strings in command argv or ad-hoc source branches.
- Backend support is capability-based. Callers must handle `UnsupportedCapability`; backend names are not a substitute for capability checks.
- Read operations run as the invoking user with locale `C`. System-scope writes are elevated and scrub loader-related environment variables.
- Tests prefer fake executables and isolated `PATH` values so argv, stderr, exit status, scope, and privilege are observable without touching a host package database.

## ANTI-PATTERNS
- Do not route a request to a different backend merely because the requested backend lacks a capability.
- Do not infer transaction identity from a visible picker label; preserve `BackendId`, scope, kind, native key, and provider metadata in the hidden token.
- Do not make optional-provider absence or failure look like success; missing optional tools may be skipped, attempted failures must remain in the outcome and affect the final status.
- Do not run package-manager matrix probes against floating image tags, host package state, host `/`, or host `sudo`.
- Do not claim a real UI, privileged transaction, or disposable matrix lane passed without running that path.

## COMMANDS
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- doctor
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- list-images
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- audit-cleanup
```

For a focused change, use `cargo test -p system-tools-core`, `cargo test -p packtide`, `cargo test -p systide`, or `cargo test -p package-manager-matrix` before the workspace gate. The matrix `all`, `single`, `probe-nix`, and VM commands require their documented external tools and network access.

## NOTES
- `packtide sysup` is a compatibility bridge to `systide`, not a second update implementation.
- Arch-only mirror, downgrade, and picker behavior must not be generalized into unrelated native backends.
- Release binaries are `target/release/packtide` and `target/release/systide`; release settings use thin LTO, stripped symbols, and abort-on-panic.
- The support contract is documented in `docs/package-manager-support.md`; update it when user-visible backend scope or command behavior changes.
