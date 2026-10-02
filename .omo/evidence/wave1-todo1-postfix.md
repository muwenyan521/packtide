# Todo 1 post-integration fixes

Base: `c021226c952c4028189a2f53ba7cd4da59b30efc` (wave1 integration HEAD)

## Changes

- `run_package_upgrade` now resolves the backend executable through `ExecutableResolver`, builds a typed `CommandPlan` with loader-variable removal, `LC_ALL=C`, and the declared privilege, and executes through `run_command_plan`. The resolver-injected API is covered by a fake `paru` consumer that captures absolute argv and locale.
- Packtide downgrade preflight resolves `downgrade` once and passes that absolute path into the transaction. Production execution uses an elevated `CommandPlan` with the same environment policy and package arguments; the debug-only fixture override remains available for integration tests.
- `ExecutableResolver` anchors relative and empty PATH entries to the construction-time working directory, so every returned executable path is absolute. Regression coverage exercises both PATH forms.

## Verification

Command: `cargo fmt --all`

Result: exit 0.

Command: `cargo test -p system-tools-core -p packtide`

Result: exit 0. `system-tools-core`: 45 unit tests, 1 fake-command integration test, 0 failures. `packtide`: 51 unit tests and 31 integration tests, 0 failures.

Command: `cargo clippy -p system-tools-core -p packtide --all-targets -- -D warnings`

Result: exit 0.

Behavioral artifacts: the fake upgrade consumer captured absolute `paru` argv (`-Su`, `--skipreview`) and `LC_ALL=C`; the packtide downgrade integration fixture captured `downgrade\nbash\n` while production code now sends only package args to the resolved plan executable. No real package transaction was run.
