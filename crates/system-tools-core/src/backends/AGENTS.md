# backend implementations

## OVERVIEW
Each module here adapts one package provider's machine output and command surface to the shared typed core contract.

## MODULES
`apt.rs`, `dnf.rs`, `zypper.rs`, `apk.rs`, and `xbps.rs` are native distribution backends. `brew.rs`, `nix.rs`, `snap.rs`, and the provider modules exposed through the parent backend contract are optional/profile or user/system providers.

## WHERE TO LOOK
- `from_path`/`from_paths`: executable resolution and provider availability.
- `parse_*`: output parser and typed error behavior.
- `*_plan` and `transaction`: exact argv, capability, scope, locale, and privilege.
- Module tests: parser edge cases and plan contracts.

## CONVENTIONS
- Keep parsing pure and testable; do not execute commands from parser functions.
- Return typed provider errors for malformed output, invalid package IDs, unsupported operations, and controlled command failures.
- Native backend detection is selected by platform identity; a decoy executable on `PATH` must not change the distribution family.
- Snap rejects local/dangerous sources, Brew is formula-only on Linux, and Nix is profile-scoped.

## ANTI-PATTERNS
- Do not silently fall back to another backend or reinterpret one provider's package identity as another's.
- Do not broaden a provider's capability set merely because its command exists.
- Do not make a write plan elevated unless the provider operation is system-scoped.

## COMMANDS
```bash
cargo test -p system-tools-core --lib
cargo test -p system-tools-core --test backend_capability_matrix
cargo test -p system-tools-core --test backend_fake_commands
cargo test -p system-tools-core --test provider_fake_path
```
