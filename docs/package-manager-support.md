English | [简体中文](package-manager-support.zh-CN.md)

# Package-manager support

This is the runtime contract for package-manager support. It records which executable is
resolved, which scope a command touches, and where the operation is exposed. The matrix runner
only checks the contract; it does not implement a second backend.

## Backend families

| Family | `BackendId` | Scope | Runtime command(s) | Update argv | Status |
| --- | --- | --- | --- | --- | --- |
| Arch native | `Pacman` | system | `pacman` (+ `sudo` for writes) | `pacman -Su` | `packtide` and `systide` |
| Debian native | `Apt` | system | `apt-get`, `apt-cache`, `dpkg-query` | `apt-get upgrade -y` | native picker and `systide` update |
| Fedora/RHEL native | `Dnf5` | system | `dnf5` (+ `rpm`) | `dnf5 upgrade -y` | native picker and `systide` update; Fedora lane |
| Fedora/RHEL legacy | `Dnf4` | system | `dnf` (+ `rpm`) | `dnf upgrade -y` | native picker and `systide` update; Rocky/Alma lanes |
| SUSE native | `Zypper` | system | `zypper` | `zypper update -y` | native picker and `systide` update |
| Alpine native | `Apk` | system | `apk` | `apk upgrade` | native picker and `systide` update |
| Void native | `Xbps` | system | `xbps-query`, `xbps-install`, `xbps-remove` | `xbps-install -Su` | native picker and `systide` update |
| Arch user | `Paru`, `Yay` | user/AUR | `paru` or `yay` | helper-specific | `packtide` Arch path |
| Flatpak | `Flatpak` | user/system | `flatpak` | `flatpak update -y` | optional `systide` step |
| Snap | `Snap` | system | `snap` | `snap refresh` | optional `systide` step; VM lane |
| Linuxbrew | `Brew` | profile | `brew` | `brew upgrade` | optional `systide` step; formulae |
| Nix | `Nix` | profile | `nix` | `nix profile upgrade .*` | optional `systide` step |

The resolver checks `PATH` and the native distribution identity. Missing optional commands are
skipped. A missing native command is an error. System operations go through the command-plan
privilege boundary, with loader-related variables removed before `sudo` runs.

## User-facing behavior

`packtide` retains its Arch-compatible interactive surface while routing detected non-Arch
native installs, removals, and update reads through the typed backend registry. `mirror-update`
and `downgrade` remain Arch-specific; they do not become generic APT/DNF/Zypper/APK/XBPS
commands. The `packtide sysup` compatibility bridge forwards to `systide` on all supported
distributions. Optional Snap/Brew/Nix rows are available
in the non-Arch native picker when the provider command is present, but are not added to the
Arch picker. `packtide sysup` only forwards to `systide`; it does not maintain another update
implementation.

`systide` detects one native backend, runs its upgrade, and then attempts present optional
backends in this order: Flatpak, Snap, Brew, Nix. It records each step, continues after an
optional failure, and exits non-zero when the native step fails. Optional failures are
warning-only: after a successful native update, they do not change exit status 0.
Native failure stops the sequence before optional providers are attempted.
AUR helpers are intentionally outside this list. Arch-specific keyring and GRUB/Waybar
finish hooks are conditional on their command/files being present.

The typed backend contract separates catalog, search, installed, details, updates, install,
remove, upgrade, downgrade, and system-upgrade capabilities. Callers must handle
`UnsupportedCapability` rather than infer support from a backend name. Snap identities are
system-scoped and reject local `.snap`/`--dangerous` sources. Brew casks are not a Linux
runtime target; the Brew lane validates formula operations only. Nix transactions are
profile-scoped and do not mutate the system store. The Arch picker remains limited to
Pacman/AUR/Flatpak; detection or matrix coverage for another backend does not imply support for
an unrelated Arch-specific command.

## Picker UI contract

The picker keeps the source label and color in one metadata layer. Rows have stable tab-separated
columns for source, package name, version/details, and the optional installed marker. The hidden
row token carries `BackendId`, `PackageScope`, package kind, native key, and provider metadata;
selection and preview must restore that typed identity rather than infer it from the visible label.
Transaction summaries include the selected scope and privilege boundary. Preview renders provider
diagnostics instead of hiding a failed details command.

Column padding uses terminal display width, including CJK names. Preview headers use a bounded
ellipsis for narrow panes and keep the package identity separate from the display text. These are
code-level contracts covered by the row/preview tests; full interactive update-flow acceptance is
listed separately below.

## Typed transaction and privilege contract

Native distribution identity selects the command generation: Fedora/RHEL with `dnf5` resolves to
`BackendId::Dnf5` and `dnf5`; otherwise the legacy lane resolves to `BackendId::Dnf4` and `dnf`.
The resolver does not select a different distribution family because a decoy executable is on
`PATH`.

Read plans run as the invoking user with locale `C`. System-scope install, remove, upgrade, and
refresh plans are `Elevated` and run through the trusted privilege runner after removing
`LD_PRELOAD` and `LD_LIBRARY_PATH`; user AUR, Brew, and Nix profile plans remain `User`. Snap is
system-scoped and its install/remove/refresh plans are also `Elevated`. The `systide` upgrade
commands are therefore `dnf5 upgrade -y` for DNF5, `dnf upgrade -y` for DNF4, and `snap refresh`
for Snap. Unsupported capabilities fail before command execution.

## Verification boundary

Unit and fake-command tests cover typed identities, exact argv, provider diagnostics, locale,
privilege, and picker rendering. They do not prove that every real backend operation works on
every distribution. Local development logs are not release evidence.

A release claiming matrix coverage must retain the source commit, locked image metadata,
JSON results, and cleanup output for the same run. Network failures, unavailable VM tools,
and incomplete guest probes remain failures or environment limitations, never passes.
Real TUI acceptance and privileged transactions need separate disposable-environment checks.

## Disposable matrix

The locked container lanes are defined in `tests/package-managers/images.lock`:

| Lane | Image family | Backend | Operations exercised |
| --- | --- | --- | --- |
| `alpine` | Alpine 3.20 | APK 2.14.4 | version, search, details, install, remove |
| `debian` | Debian 12-slim | APT 2.6.1 | version, update, search, details, install, remove |
| `ubuntu` | Ubuntu 24.04 | APT 2.8.3 | version, update, search, details, install, remove |
| `fedora` | Fedora 41 | DNF5 5.2.17.0 | list, search, details, install, remove |
| `rocky` / `alma` | EL9 | DNF4 4.14.0 | list, details, install, remove |
| `opensuse` | Leap 15.6 | Zypper 1.14.94 | search, details, install, remove |
| `void` | Void musl | XBPS 0.59.1 | sync, list, search, details, install, remove |

`all` adds pinned Homebrew and Nix images plus the Snap VM. Containers run with Podman
`--rm`, a generated `pm-matrix-*` name, and a bridge network. They are disposable and do
not touch the host package database. `audit-cleanup` must be run after an interrupted run;
it checks containers, QEMU processes, temporary overlays, and generated Podman networks.

The Snap VM is intentionally separate because Snap needs a running `snapd` and access to
the Snap Store. It uses the immutable provenance in
`tests/package-managers/ubuntu-cloud-image.lock`, QEMU/KVM, and a cloud-init seed. The VM
lane needs guest network egress; a host firewall, offline CI, missing `/dev/kvm`, or missing
`cloud-localds`/`xorriso` makes the lane unavailable. That result is recorded as an
environment limitation and does not change the host backend support contract.

## Commands

From the repository root:

```bash
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- doctor
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- list-images
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- all --evidence evidence/package-manager-matrix.jsonl
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- single --backend fedora --evidence evidence/fedora.json
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- probe-nix --evidence evidence/nix.json
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- verify-cloud-image
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- vm-run
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- vm-interrupt-test
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- audit-cleanup
```

Use `cargo test -p package-manager-matrix` for parser, lock, timeout, and cleanup tests.
Matrix commands may need network access to pull pinned images and package repositories;
never substitute floating image tags for the locked digest without updating the lock and
captured evidence together.
