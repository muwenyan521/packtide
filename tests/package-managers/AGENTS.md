# package-manager test data

## OVERVIEW
This directory contains locked image metadata and deterministic command-output fixtures for package-manager matrix tests; it contains no implementation code.

## STRUCTURE
- `images.lock`: registry, tag, digest, architecture, package-manager, and version for disposable container lanes.
- `ubuntu-cloud-image.lock`: pinned Snap VM image provenance.
- `fixtures/<backend>/`: captured catalog, installed, search, update, details, or command outputs consumed by tests.
- `fixtures/README.md`: fixture safety and format notes.

## CONVENTIONS
- Fixture files model provider output, including whitespace, optional fields, and non-zero stderr cases. Preserve the format under test.
- Lock changes require updating the corresponding digest/provenance and rerunning image validation; do not silently retag an image.
- Fixtures must be deterministic and self-contained. They must not invoke `sudo`, write `/`, or depend on host package state/network.
- Add a fixture only with a test that consumes it; keep provider-specific files under that provider's directory.

## ANTI-PATTERNS
- Do not use live repository output as an unpinned fixture.
- Do not remove unknown/future fields from JSON/XML/TSV fixtures merely to simplify a parser test.
- Do not encode secrets, personal paths, credentials, or host-specific state in test data.

## COMMANDS
```bash
cargo test -p system-tools-core
cargo test -p packtide
cargo test -p package-manager-matrix
```
