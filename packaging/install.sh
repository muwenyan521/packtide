#!/usr/bin/env bash
set -euo pipefail

root=${DESTDIR:-}
prefix=${PREFIX:-/usr}
bindir="$root$prefix/bin"
install -Dm755 target/release/packtide "$bindir/packtide"
install -Dm755 target/release/systide "$bindir/systide"
ln -sfn packtide "$bindir/ptd"
ln -sfn systide "$bindir/suu"
install -Dm644 man/packtide.1 "$root$prefix/share/man/man1/packtide.1"
install -Dm644 man/systide.1 "$root$prefix/share/man/man1/systide.1"
install -Dm644 man/packtide.zh-CN.1 "$root$prefix/share/man/zh_CN/man1/packtide.1"
install -Dm644 man/systide.zh-CN.1 "$root$prefix/share/man/zh_CN/man1/systide.1"
install -Dm644 completions/packtide.bash "$root$prefix/share/bash-completion/completions/packtide"
install -Dm644 completions/systide.bash "$root$prefix/share/bash-completion/completions/systide"
install -Dm644 completions/_packtide "$root$prefix/share/zsh/site-functions/_packtide"
install -Dm644 completions/_systide "$root$prefix/share/zsh/site-functions/_systide"
install -Dm644 completions/packtide.fish "$root$prefix/share/fish/vendor_completions.d/packtide.fish"
install -Dm644 completions/systide.fish "$root$prefix/share/fish/vendor_completions.d/systide.fish"
for file in LICENSE NOTICE.md; do
  test ! -e "$file" || install -Dm644 "$file" "$root$prefix/share/licenses/packtide/$file"
done
