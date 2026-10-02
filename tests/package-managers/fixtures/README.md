# Package-manager matrix fixtures

Fixtures in this directory are intentionally inert. Harness tests may prepend a
temporary directory containing fake `podman`, `qemu-system-x86_64`, or package
manager commands to `PATH`; no fixture invokes `sudo`, touches `/`, or uses the
host network. The lock file is parsed without evaluating shell/TOML expressions.

