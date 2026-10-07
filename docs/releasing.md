English | [简体中文](releasing.zh-CN.md)

# Release checklist

The current public release target is `0.1.2`. A local build is not a release, a reproducible-build claim, or proof that every distro path works.

1. Pick the source commit. Review the diff and [source notices](../NOTICE.md); keep the workspace version, tag, package recipe, and artifact names in sync.
2. Run the checks in [CONTRIBUTING.md](../CONTRIBUTING.md). Exercise CLI help, TUI preview/cancel, man pages, completions, and runtime dependencies.
3. Run the locked disposable package-manager lanes that apply. Keep the commit, toolchain, commands, JSON results, and cleanup output together. An unavailable lane or repository/network failure is not a pass.
4. Build both GNU and static musl Linux archives with the release workflow's baseline environment. Include both binaries, `ptd`/`suu`, the installer, man pages, completions, `LICENSE`, `NOTICE.md`, provenance, and a SHA-256 checksum. Install each under a temporary prefix and check the uninstall scope.
5. Review those artifacts, then create and push the matching version tag. Publishing requires explicit maintainer approval.
6. Download the uploaded archive and checksum, verify both, and keep their hashes with the source commit. Announce only artifacts that can be downloaded and checked.

`commands/release-sha256.txt` is an old local-build record, not the checksum file for a new release. Generate checksums from the final artifacts. Thin LTO, eight codegen units, stripped symbols, and abort-on-panic are release settings; they do not establish bit-for-bit reproducibility.

## Release notes

Record the behavior change, supported capability and scope paths, runtime requirements, tested scenarios, and known limits. Call out the difference between the multi-distro native backends and Arch-only mirror/downgrade features. On supported distributions, `packtide sysup` forwards to `systide`. Missing optional tools are skipped; native failure stops the sequence; an attempted optional failure is a warning and later optional providers are still tried. After native success, optional failure leaves exit status 0.
