# packtide mirror update

## OVERVIEW
The mirror-update tree implements the Arch-only reflector workflow with staged transactions and rollback support.

## WHERE TO LOOK
- `mirror_update.rs`: command entry, country/region fallback, and user-facing diagnostics.
- `transaction.rs`: transaction state machine and orchestration.
- `transaction/staged.rs`: stage and commit operations.
- `transaction/rollback.rs`: restore path after an interrupted or failed transaction.
- `tests.rs` and `transaction/tests.rs`: fake command and rollback behavior.

## CONVENTIONS
- Keep mirror update separate from generic native package-manager operations.
- Treat staging and commit as distinct phases; rollback must remain possible until the commit boundary.
- Missing `reflector` and failed subprocesses are explicit operation errors, not successful no-ops.

## ANTI-PATTERNS
- Do not mutate the live mirror configuration before the staged plan is ready.
- Do not swallow a failed rollback or report a partially applied transaction as success.
- Do not broaden this path to APT, DNF, Zypper, APK, or XBPS.

## COMMANDS
```bash
cargo test -p packtide mirror_update
cargo test -p packtide --test flatpak_remove mirror_update
```
