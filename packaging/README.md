English | [简体中文](README.zh-CN.md)

# Packaging

The release contains `packtide` and `systide`. Supported installation paths:

* Download the published Linux tarball, put both binaries on `PATH`, and install `man/` and `completions/` into the matching system directories.
* Arch Linux: from a checkout, run `makepkg -si` in `packaging/packtide/`. The recipe is
  checkout-local for now; an official package submission should switch it to a tagged source archive.
* From a checkout: run `cargo install --path crates/packtide --locked` and `cargo install --path crates/systide --locked`. This installs the binaries; man pages and shell completions remain in this tree.

[`the release workflow`](https://github.com/muwenyan521/packtide/actions/workflows/release.yml) builds the tarball for `x86_64-unknown-linux-gnu` and publishes a SHA-256 sidecar. Runtime dependencies and backend support are in the repository README.
