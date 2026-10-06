# system-tools-core

## OVERVIEW
`system-tools-core` is the shared typed boundary for package identities, backend capabilities, parsers, command plans, executable resolution, caching, and privileged execution.

## STRUCTURE
```text
src/
├── backend.rs              # BackendId, identity, capabilities, read/write contracts
├── backends/               # provider-specific parsers and command plans
├── registry.rs             # built-in backend dispatch and identity routing
├── platform.rs             # /etc/os-release and native backend detection
├── plan.rs, transaction.rs # typed argv, scope, privilege, upgrade plans
├── command.rs              # subprocess execution and environment scrubbing
├── executable.rs           # PATH-based executable resolution
├── package.rs              # source/kind/action identity helpers
└── cache.rs                # atomic TTL cache and refresh lock
tests/                      # fake executable and backend matrix contracts
```

## CODE MAP
- `BackendId`, `PackageIdentity`, `CapabilitySet`, `ReadOperation`, `WriteOperation` in `src/backend.rs` define the public contract.
- `BackendRegistry` in `src/registry.rs` routes reads/writes and rejects identity mismatches before spawning a command.
- `CommandPlan` in `src/plan.rs` carries program, argv, locale, removed environment variables, and privilege.
- `PrivilegeRunner` and `run_command_plan` in `src/command.rs` are the subprocess boundary.
- `detect_native_backend*` in `src/platform.rs` selects the native distribution without provider substitution.

## CONVENTIONS
- Every backend exposes only the capabilities it can execute; unsupported operations fail before command execution.
- Read plans run in locale `C` as the invoking user. System-scope writes use `Elevated`; user/AUR/profile writes remain `User`.
- Tests assert exact argv, status, stderr, scope, and identity using temporary fake executables and resolver paths.
- Provider parsers must tolerate documented output variation while preserving package identity and typed errors.
- Cache writes are atomic and refreshes are lock-protected; partial writes must not become valid cache entries.

## ANTI-PATTERNS
- Do not let a backend name imply unsupported operations, scope, or privilege.
- Do not resolve commands from an arbitrary display string or permit cross-backend identity tampering.
- Do not hide stderr, non-zero status, or provider diagnostics behind a generic empty result.
- Do not use host package databases or privileged real commands in fake-path tests.

## COMMANDS
```bash
cargo test -p system-tools-core
cargo test -p system-tools-core --test core_read_matrix
cargo test -p system-tools-core --test write_execution_matrix
cargo test -p system-tools-core --test read_failure_matrix
cargo clippy -p system-tools-core --all-targets -- -D warnings
```
