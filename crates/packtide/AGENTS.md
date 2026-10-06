# packtide

## OVERVIEW
`packtide` is the package-management TUI/CLI. It preserves the Arch-compatible surface while routing non-Arch native operations through `system-tools-core`.

## STRUCTURE
```text
src/
├── app.rs, cli.rs, commands.rs # process entry and subcommand dispatch
├── sources.rs, sources/        # provider catalog/search/installed reads
├── install.rs, remove.rs       # picker-driven transactions
├── check_updates.rs, downgrade.rs, mirror_update.rs # update and Arch flows
├── transaction.rs, model.rs    # typed operation input and update models
├── ui/                         # fzf rows, preview, colors, query reload
└── locales/                    # embedded English/Chinese text
tests/                          # fake-command and provider matrix integration tests
```

## WHERE TO LOOK
- `src/app.rs`: selects the native backend and validates required commands.
- `src/sources.rs`: builds provider rows; optional providers are queried independently.
- `src/source_metadata.rs`: source labels, scope, hidden identity, and legacy mapping.
- `src/install.rs`, `remove.rs`, `transaction.rs`: route selection to typed writes.
- `src/ui/rows.rs`, `preview.rs`: stable tab columns, hidden tokens, display-width handling, and diagnostics.
- `tests/`: use `cargo test -p packtide --test <name>` for a focused integration lane.

## CONVENTIONS
- The fzf row's visible label is presentation only. Selection must round-trip the hidden typed identity.
- Arch picker paths remain Pacman/AUR/Flatpak. Non-Arch native paths use the detected backend and capability checks.
- `--no-ai` is accepted as a compatibility no-op; no AI provider is invoked.
- Locale selection is controlled by `PACKTIDE_UI_LANG=auto|zh|en` and locale environment variables.

## ANTI-PATTERNS
- Do not add a second implementation of system update orchestration; `sysup` forwards to `systide`.
- Do not silently convert provider/read/preview failures into cancellation or an empty catalog.
- Do not invoke a transaction before picker confirmation, and do not mix package scopes under one privilege assumption.
- Do not derive a native package key from localized or colored display text.

## COMMANDS
```bash
cargo test -p packtide
cargo test -p packtide --test flatpak_remove
cargo test -p packtide --test native_install_matrix
cargo run -p packtide -- --help
```
