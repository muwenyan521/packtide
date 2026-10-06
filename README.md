<div align="right">
  English | <a href="README.zh-CN.md">简体中文</a>
</div>

<div align="center">

# packtide · systide

**Package and system maintenance for Linux**

`packtide` handles packages. `systide` handles system updates. Both use the same typed
backend layer, so the command that runs is explicit and testable.

[![License: MPL-2.0](https://img.shields.io/badge/license-MPL--2.0-blue.svg)](LICENSE)
[![Rust 1.88+](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org/)

</div>

> The interaction model was informed by the behavior of the original projects. This
> repository is an independent Rust implementation.

## Contents

- [What it does](#what-it-does)
- [Install](#install)
- [Supported backends](#supported-backends)
- [Quick start](#quick-start)
- [Runtime requirements](#runtime-requirements)
- [Development](#development)
- [Verification](#verification)
- [License](#license)

## Commands

### `packtide`

`packtide` opens a searchable picker, shows a preview, and runs a package transaction after
you confirm it. The selected package keeps its backend and scope identity when display text
is localized or reformatted.

- Install, remove, update, and inspect packages through the detected native backend.
- Search with a cached result first; missing or stale query results are refreshed
  asynchronously so typing stays responsive.
- Keep optional Flatpak, Snap, Homebrew, and Nix results visible when their tools exist.
- Use Arch-specific mirror and downgrade workflows only where those capabilities exist.
- Use `packtide sysup` as the portable forwarding command for `systide`.

### `systide`

`systide` runs the detected native update first, then tries optional providers installed on
the machine. It prints each step and keeps provider errors in the output.

- Detect the native package manager from the distribution and available tools.
- Run native updates first; a native failure stops the sequence.
- Attempt available optional providers in a stable order: Flatpak, Snap, Brew, Nix.
- Treat optional failures as warnings after native success while continuing with later providers.
- Keep Arch-only news, mirror, snapshot, bootloader, and status hooks conditional.

## Install

### Release archive

Download the [`x86_64-unknown-linux-gnu` release archive](https://github.com/muwenyan521/packtide/releases),
check its SHA-256 sidecar, and put the two binaries on `PATH`. It also contains manpages,
shell completions, `LICENSE`, `NOTICE.md`, and a small provenance file.

### From a checkout

```bash
cargo install --path crates/packtide --locked
cargo install --path crates/systide --locked
```

For a system-wide staged install, including manpages and completions:

```bash
DESTDIR="$PWD/stage" PREFIX=/usr packaging/install.sh
```

### Arch packaging

`packaging/packtide/PKGBUILD` works from a checkout. Before submitting it to a public package
repository, switch it to the tagged release source expected by that repository.

## Supported backends

| Backend | Native package operations | System update | Additional scope |
| --- | --- | --- | --- |
| Pacman | Install, remove, search, updates, downgrade | `pacman -Su` | AUR helpers, mirror workflow |
| APT | Install, remove, search, updates | `apt-get upgrade -y` | Debian and Ubuntu |
| DNF 5 | Install, remove, search, updates | `dnf5 upgrade -y` | Fedora and RHEL-family systems |
| DNF 4 | Install, remove, search, updates | `dnf upgrade -y` | Rocky and Alma Linux |
| Zypper | Install, remove, search, updates | `zypper update -y` | openSUSE |
| APK | Install, remove, search, updates | `apk upgrade` | Alpine Linux |
| XBPS | Install, remove, search, updates | `xbps-install -Su` | Void Linux |
| Flatpak | Optional package rows | Optional update | User or system scope |
| Snap | Optional package rows | Optional update | System scope |
| Brew | Optional formula rows | Optional update | Linuxbrew user scope |
| Nix | Optional profile rows | Optional update | Profile scope |

Each operation checks its own capability. Finding a package manager does not make unsupported
operations available. The [support contract](docs/package-manager-support.md) lists the exact
commands and scopes.

## Quick start

```bash
packtide install ripgrep
packtide check-updates
packtide remove
systide
```

Useful flags:

```text
packtide install [QUERY...] [--refresh] [-y] [--no-ai]
packtide check-updates [--refresh]
packtide remove [QUERY...]
systide [--list] [--ui-lang auto|zh|en] [--news-source SOURCE] [--count N]
```

## Runtime requirements

- `fzf >= 0.74.0` for the interactive picker.
- The native package-manager commands for the detected distribution.
- `sudo` for system-scope transactions.
- Optional provider commands only when their rows or updates are wanted.

The locale environment selects the language. Override it with
`PACKTIDE_UI_LANG=auto|zh|en` for `packtide` or `--ui-lang auto|zh|en` for `systide`.

## Development

Read the scoped [AGENTS.md](AGENTS.md) before changing a subsystem. Keep backend identities,
command plans, privilege routing, and fake-command tests explicit. Tests must not touch the
host package database.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release --locked
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for test selection and [docs/releasing.md](docs/releasing.md)
for the release checklist. The package-manager matrix uses locked disposable images; a lane
that cannot run stays incomplete.

## Verification

Tests cover parser contracts, exact argv, scope and privilege, localized rows, query
cancellation, provider diagnostics, and typed identity round trips. Fake commands do not prove
that a real distribution accepted a privileged transaction; run the package-manager and VM
lanes in a disposable environment before claiming that.

## License

This workspace is distributed under the [Mozilla Public License 2.0](LICENSE). See
[NOTICE.md](NOTICE.md) for source-provenance and third-party relationship notices.
