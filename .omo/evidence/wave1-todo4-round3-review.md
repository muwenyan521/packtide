# Wave 1 Todo 4 round 3 fix evidence

Worktree: /home/wangxianming/dev/shorin-contrib-wt/wave1-todo4-fix3
Base: 110332f (package-manager-expansion/wave1-integration)

## Changes

- system-tools-core package_upgrade_command and run_package_upgrade now take
  BackendId and return Result with BackendError. Unknown or unsupported backends
  return UnsupportedCapability; no manager string is interpolated into a
  guessed -Syu command.
- systide detects a typed native backend and passes it through the update
  caller. Existing Arch/Pacman, paru, and yay argv and privilege behavior
  remains explicit.
- systide --list only probes checkupdates, paru, and yay when native detection
  reports Arch/Pacman with the update capability. Flatpak remains independent.
- packtide profile scope locale rendering is covered by a production-path test
  for English and Chinese labels. Existing locale keys remain in use.

## Verification

Commands run from the worktree:

  cargo fmt --all
  cargo clippy -p system-tools-core -p systide -p packtide --all-targets -- -D warnings
  PASS

  cargo test -p systide --bin systide
  PASS: 17 passed, 0 failed

  cargo test -p system-tools-core
  PASS: 41 unit tests, 1 integration test, 0 failed

  cargo test -p packtide --bin packtide
  PASS: 51 passed, 0 failed

  git diff --check
  PASS

The workspace-wide clippy command was also attempted. It is blocked by two
pre-existing tools/package-manager-matrix/src/main.rs lints (collapsible_if at
line 589 and let_and_return at line 649); the targeted crate clippy command is
clean.

## Fake provider scenarios

The systide UI unit tests use an isolated executable directory and marker files:

- ui::tests::non_arch_native_list_skips_arch_only_providers injects
  NativeBackend::Apt. Fake checkupdates and paru marker files remain absent,
  while the fake Flatpak marker is present and rows contain no Pacman/AUR source.
- ui::tests::arch_native_list_preserves_pacman_and_aur_providers injects
  NativeBackend::Pacman. All three fake provider markers are present and rows
  contain Pacman, AUR, and Flatpak sources.

No host files, /etc/os-release, package databases, or privileged commands are
modified by these scenarios.
