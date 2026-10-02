# Wave 1 Todo 3 backend migration evidence

Worktree: `wave1-task3`

## Static and unit evidence

- `cargo fmt --all` exited 0.
- `cargo test -p system-tools-core -p packtide --lib` exited 0 for the core target (24 tests).
- `cargo test -p packtide` exited 0: 50 binary unit tests and 31 integration tests.
- `cargo clippy -p system-tools-core -p packtide --all-targets -- -D warnings` exited 0.
- `git diff --check` exited 0.

The typed `BuiltinBackend` contract now emits `paru|yay -S/-Rns`, `flatpak uninstall`, and rejects Flatpak downgrade. `packtide` transaction execution consumes the resulting structured `CommandPlan`, preserving application IDs and AUR repository targets as `PackageId` values.

Existing fake-command integration scenarios in `crates/packtide/tests/flatpak_remove.rs` observed the exact transaction argv, including Flatpak application ID and AUR/repository targets; all 31 scenarios passed.

## Manual surface status

The release `packtide` binary was not launched against a real Arch package database in this non-Arch workspace. No real package transaction was attempted. The integration harness provides the non-transactional picker/preview/cancel coverage and fake PATH capability-error coverage.
