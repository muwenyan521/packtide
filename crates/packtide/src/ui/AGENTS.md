# packtide UI

## OVERVIEW
This directory owns the fzf-based picker presentation, preview execution, color metadata, and query reload helper.

## WHERE TO LOOK
- `rows.rs`: `PackageRow`, stable tab-separated columns, hidden identity token encoding/decoding, and terminal display-width padding.
- `preview.rs`: provider details/diagnostics and narrow-pane title truncation.
- `fzf.rs`: picker process setup, preview invocation, and cancellation/error mapping.
- `colors.rs`: source-specific colors and fzf color options.
- `query_reload.sh`: the small helper used by the fzf query-refresh path.

## CONVENTIONS
- Keep the hidden token typed: backend, scope, package kind, native key, and provider metadata must survive a render/parse round trip.
- Visible columns are stable and width-aware, including CJK names; use display width rather than byte or scalar count.
- A failed details command is rendered as a diagnostic. It must not become a fabricated empty preview.
- Cancellation, an empty catalog, and a provider failure are distinct outcomes for callers.

## ANTI-PATTERNS
- Do not parse identity back out of a localized/colorized label.
- Do not change column order without updating row/preview tests and all fzf consumers.
- Do not use shell string concatenation for untrusted package arguments; pass structured argv to the command boundary.

## COMMANDS
```bash
cargo test -p packtide --lib
cargo test -p packtide --test flatpak_remove
cargo test -p packtide --test real_path_matrix
```
