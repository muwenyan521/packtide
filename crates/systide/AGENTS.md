# systide

## OVERVIEW
`systide` is the system update orchestrator. It detects one native backend, runs the native update, then attempts available optional providers while preserving diagnostics. Native failure is fatal; optional failure is reported but does not turn a successful native update into a failed command.

## STRUCTURE
```text
src/
├── main.rs, cli.rs          # argument parsing and top-level flow
├── flow.rs, messages.rs     # confirmation, locale, and user-facing output
├── operations.rs, update.rs # backend detection, plan/order, execution result
├── ui.rs                    # update list and picker rows
├── news.rs                  # Arch news retrieval and rendering
├── mirror.rs, snapshot.rs, finish.rs # Arch conditional finish hooks
└── locales/                 # embedded English/Chinese text
tests/                       # CLI and orchestration integration contracts
```

## WHERE TO LOOK
- `src/main.rs`: `--list-data`, language selection, privilege gate, confirmation, and orchestration entry.
- `src/update.rs`: native-first ordering, optional provider attempts, and outcome aggregation.
- `src/operations.rs`: native backend detection and manager invocation.
- `src/ui.rs`: localized row rendering and cancellation behavior.
- `tests/cli.rs`, `tests/orchestration.rs`: fake `PATH` integration tests.

## CONVENTIONS
- `--ui-lang auto|zh|en` uses `LC_ALL`, then `LC_MESSAGES`, then `LANG`.
- Optional providers are attempted only when their executable exists; an attempted failure is retained in diagnostics while the native result remains the exit-status contract.
- AUR helpers are not part of the automatic update list.
- Arch-only news, mirror, snapshot, GRUB, and Waybar hooks remain conditional and must not run for unrelated native backends.

## ANTI-PATTERNS
- Do not run optional providers before a native update or hide their failure behind a successful native result.
- Do not report an empty `--list-data` result as success when no source produced rows.
- Do not bypass the privilege check for system-scope updates.
- Do not treat cancellation, unavailable provider, and provider execution failure as the same state.

## COMMANDS
```bash
cargo test -p systide
cargo test -p systide --test cli
cargo test -p systide --test orchestration
cargo run -p systide -- --help
```
