# Release artifacts evidence

Date: 2026-10-06

## Delivered

- `.github/workflows/ci.yml`: format, clippy, workspace tests, and release build on pull requests and `master` pushes.
- `.github/workflows/release.yml`: tagged `v*` release packaging for `x86_64-unknown-linux-gnu`, tarball, SHA-256 sidecar, and GitHub Release upload.
- `man/packtide.1`, `man/systide.1`: options and command references matching the current clap definitions.
- `completions/`: Bash, Zsh, and Fish completions for both binaries.
- `packaging/README.md`: tarball, Arch PKGBUILD, and source installation entry points.
- `packaging/packtide/PKGBUILD`: installs binaries, man pages, and shell completions.
- `README.md`: links the release and installation entry points.

## Verification

| Scenario | Invocation | Observable result |
| --- | --- | --- |
| Release compilation | `cargo build --workspace --release --locked` | exit 0; both `target/release/packtide` and `target/release/systide` built |
| CLI surface | `target/release/packtide --help`; `target/release/systide --help` | exit 0; output lists the documented commands/options |
| Shell completion syntax | `shellcheck completions/*.bash` | exit 0 |
| Manpage syntax | `for f in man/*.1; do groff -man -Tutf8 "$f" >/dev/null; done` | exit 0 |
| Diff whitespace | `git diff --check` | exit 0 |

## Remaining risks

- `cargo test --workspace` is currently blocked by an existing compile failure at `crates/systide/src/update.rs:262` (`UpdateOutcome::exit_code()` does not exist); no core behavior was changed here.
- YAML syntax validation was not run because this environment has neither Python PyYAML nor Ruby installed. The workflow files are intentionally small and use standard GitHub Actions syntax.
- Release workflow publishes only the Linux x86_64 tarball; other targets are not claimed.
