# package-manager-matrix

## OVERVIEW
This binary is a disposable integration runner for locked package-manager images and the separate Snap VM lane; it is not a second backend implementation.

## STRUCTURE
- `src/main.rs`: command dispatch, lock parsing, Podman probes, QEMU/Snap flow, JSON rendering, timeout and cleanup handling.
- `tests/signal_cleanup.rs`: interruption cleanup contract.
- `tests/package-managers/images.lock`: immutable container image metadata.
- `tests/package-managers/ubuntu-cloud-image.lock`: Snap VM provenance.
- `tests/package-managers/fixtures/`: deterministic provider output used by tests.

## COMMANDS
```bash
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- doctor
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- list-images
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- single --backend debian
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- probe-nix
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- all
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- vm-run
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- vm-interrupt-test
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- audit-cleanup
cargo test -p package-manager-matrix
```

## CONVENTIONS
- Image references come from the lock files and must be validated before a probe.
- Container lanes use disposable Podman resources and an isolated network namespace; Snap uses a separate QEMU/KVM lane.
- Every probe emits structured JSON with lane status, command output, and cleanup state.
- SIGINT/SIGTERM paths must reap children and remove temporary overlays/networks before returning.

## ANTI-PATTERNS
- Do not replace locked digests with floating tags or claim a network/TLS failure is backend support.
- Do not run probes with host `sudo`, write host `/`, or reuse the host package database.
- Do not skip cleanup after an interrupted or failed lane.
- Do not treat `unavailable`, missing VM tooling, repository failure, or guest egress failure as a pass.
