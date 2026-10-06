# Packaging

The release contains two binaries, `packtide` and `systide`. The supported
installation paths are:

* Download the Linux tarball from the GitHub Releases page, copy both binaries
  to a directory on `PATH`, and install the files in `man/` and `completions/`
  into the matching system directories.
* Arch Linux: build and install `packaging/packtide/PKGBUILD` with `makepkg -si`.
* From a checkout: run `cargo install --path crates/packtide --locked` and
  `cargo install --path crates/systide --locked`. This installs the binaries;
  shell completions and man pages remain available from this directory.

The release tarball is built by `.github/workflows/release.yml` for
`x86_64-unknown-linux-gnu` and includes a SHA-256 sidecar file. Runtime
dependencies and backend support are documented in the repository README.
