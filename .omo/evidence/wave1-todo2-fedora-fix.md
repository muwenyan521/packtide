# Wave 1 Todo 2 Fedora dnf5 lane

This worktree is based on `cb3f90451fce4b2a674f22013b1f77b52a0ea922` and uses the locked Fedora 41 digest from `tests/package-managers/images.lock`.

## Change

The Fedora `dnf5` disposable-container probe now performs a real transaction. It rejects a preinstalled `hello`, runs `dnf5 install -y --setopt=install_weak_deps=False hello`, verifies installation with `rpm -q hello`, runs `dnf5 remove -y hello`, and verifies removal with a final `rpm -q hello` failure check. The existing `podman run` uses the digest-pinned image, `run_bounded` enforces the 90 second timeout, and `CleanupGuard` plus `cleanup_check` record cleanup state. Captured stdout and stderr remain in the rendered probe for failure diagnostics.

## Verification

`cargo fmt --all` exited 0.

`cargo test -p package-manager-matrix` exited 0: 9 unit tests and 1 signal cleanup integration test passed.

`cargo clippy -p package-manager-matrix --all-targets -- -D warnings` exited 0.

Real disposable lane command:

`target/debug/package-manager-matrix single --backend fedora`

Exited 0. The captured artifact reports `lane_status":"pass"`, the locked Fedora digest, `dnf5` version `5.2.17.0`, `BEFORE`, `INSTALL`, `rpm` output `hello-2.12.2-1.fc41.x86_64`, `REMOVE`, `AFTER`, and `cleanup":true`. No package transaction ran on the Arch host.

The matrix doctor was also run. It found `podman`, QEMU, and KVM, but reported the pre-existing host dependency `cloud-localds` as missing; this only blocks the unrelated Snap VM lane.
