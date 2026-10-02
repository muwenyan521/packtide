# Todo2 matrix runner evidence

Base: `ee30d03`; worktree: `wave1-todo2-final-runner`.

`cargo test --manifest-path tools/package-manager-matrix/Cargo.toml` exited 0 (8 unit tests and 1 SIGTERM cleanup test).

`single --backend alpine --evidence alpine.jsonl` exited 0. The disposable locked Alpine container reported `apk-tools 2.14.4`, then emitted LIST, DETAILS, INSTALL and REMOVE sections for `busybox`/`curl`; the JSON receipt has `cleanup:true`.

`single --backend debian --evidence debian.jsonl` exited 0. The disposable locked Debian container reported `apt 2.6.1`, searched and showed `hello`, installed it, removed it, and emitted `cleanup:true`.

`doctor` exits 1 with `cloud_localds:"MISSING"` on this host. This is an honest prerequisite failure; the diagnostic now names the executable consumed by `vm-run` rather than the unrelated `cloud-init` command.

`audit-cleanup` exits 0 with no matching containers, QEMU processes, temp overlays or matrix networks.
