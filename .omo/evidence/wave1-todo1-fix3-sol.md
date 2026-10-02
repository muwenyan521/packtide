# Todo1 round3 fix evidence

- Worktree: `/home/wangxianming/dev/shorin-contrib-wt/wave1-todo1-fix3`
- Base: `110332f`
- `cargo fmt --all`: PASS
- `cargo test -p system-tools-core`: PASS, 42 unit tests, fake-command integration test, and doc tests
- `cargo test -p packtide --test flatpak_remove`: PASS, 31 tests
- `cargo clippy -p system-tools-core --all-targets -- -D warnings`: PASS
- `git diff --check`: PASS

Behavior covered:

- `write_plan_uses_absolute_path_from_injected_resolver` records that a trusted injected resolver produces an absolute executable path.
- `run_command_plan` consumes `env_remove`, `LC_ALL`, and `CommandPrivilege`; user plans execute directly and elevated plans route through the trusted `PrivilegeRunner`.
- Packtide fake-command transaction tests observe exact Flatpak argv while resolving the fake executable path from the test PATH.
- Flatpak `SystemUpgrade` now reports `CapabilitySet::SYSTEM_UPGRADE`; backend identity classification test uses explicit expected mappings rather than an exhaustive tautology.
