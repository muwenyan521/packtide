English | [简体中文](CONTRIBUTING.zh-CN.md)

# Contributing

Keep changes focused. In the description, name the user-visible behavior, affected backend, and failure boundary. Read the scoped `AGENTS.md` before changing a subsystem.

## Checks

Start with the smallest suite that covers the change. Before a release or cross-module handoff, run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release --locked
```

Backend changes need tests for exact argv, stderr/status, scope, locale, and privilege. UI changes need a real TUI preview/cancel run, not just row snapshots. Use disposable containers or VMs for package transactions; never install into or remove from the host package database. See the [support contract](docs/package-manager-support.md) for matrix commands.

For each claim, record the invocation, source commit, observable result, and retained artifact. Say which checks are fake/unit tests and which exercise a real backend. Report unavailable lanes as unavailable. Public reports must not contain credentials, personal data, or machine-specific paths.

## Source and license

The project is distributed under [MPL-2.0](LICENSE). Contributions must be compatible with it. Preserve notices and check the license of third-party code before adding it. Historical reference projects are not permission to copy their code or text; see [NOTICE.md](NOTICE.md). Update the support contract and user docs when capabilities or failure behavior changes.
