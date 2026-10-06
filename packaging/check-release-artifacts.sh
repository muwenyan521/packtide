#!/usr/bin/env bash
set -euo pipefail

command -v fzf >/dev/null || { echo 'fzf is required' >&2; exit 1; }
fzf_version=$(fzf --version | awk '{print $1}')
printf '%s\n' "$fzf_version" | awk -F. '{ if (($1+0) < 0 || (($1+0) == 0 && ($2+0) < 74)) exit 1 }' || {
  echo "fzf >= 0.74.0 required (found $fzf_version)" >&2; exit 1;
}
fzf_help=$(fzf --help)
for flag in --id-nth --track --preview-window --bind; do
  grep -q -- "$flag" <<<"$fzf_help" || { echo "fzf missing $flag" >&2; exit 1; }
done

grep -qi 'pressing Enter' man/systide.1
packtide_help=$(target/release/packtide --help)
systide_help=$(target/release/systide --help)
for command in check-updates install remove mirror-update downgrade sysup; do
  grep -q "  $command" <<<"$packtide_help" || { echo "packtide help missing $command" >&2; exit 1; }
done
for option in --list --ui-lang --news-source --count; do
  grep -q -- "$option" <<<"$systide_help" || { echo "systide help missing $option" >&2; exit 1; }
done
for file in man/*.1 completions/*; do test -s "$file"; done
echo "release artifacts and fzf $fzf_version are compatible"
