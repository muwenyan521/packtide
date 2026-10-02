# Package-manager matrix Todo 2

- `cargo fmt --all -- --check`: PASS.
- `cargo test -p package-manager-matrix`: PASS (2 unit tests: digest validation and lock parsing).
- `cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- doctor`: PASS; Podman 6.1.2, QEMU 11.1.1, `/dev/kvm` present; cloud-init reported missing (Snap VM lane deferred).
- `... -- list-images`: PASS; five locked entries emit registry/tag/manifest digest/architecture/package-manager/version JSON fields.
- Alpine probe: PASS using immutable `docker.io/library/alpine@sha256:c64c...83b5`, `apk --print-arch` returned `x86_64`, `--network none`, `--rm`, cleanup true.
- Invalid digest: PASS negative case; exits 1, emits structured JSON, no Podman container remains (`podman ps -a` had no `pm-matrix-*`).
- Ubuntu cloud image URL and SHA256 are recorded in `tests/package-managers/ubuntu-cloud-image.lock`; no VM boot or download was attempted.

The harness is rootless and bounded to 90 seconds per child process. It never invokes sudo, mounts host paths, or uses host network in the probe.
