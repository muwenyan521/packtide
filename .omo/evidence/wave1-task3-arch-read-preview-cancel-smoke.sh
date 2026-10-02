#!/usr/bin/env bash
set -Eeuo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
binary=${PACKTIDE_RELEASE_BIN:-$root/target/release/packtide}
evidence=${EVIDENCE_OUT:-$root/.omo/evidence/wave1-task3-arch-read-preview-cancel-smoke.log}
tmp=$(mktemp -d "${TMPDIR:-/tmp}/packtide-arch-smoke.XXXXXX")
bin=$tmp/bin
cache=$tmp/cache
mkdir -p "$bin" "$cache/packtide/aur" "$(dirname "$evidence")"
export PACKTIDE_SMOKE_ROOT=$tmp

cleanup() {
    status=$?
    if [ -d "$tmp" ]; then
        find "$tmp" -depth -delete || status=1
    fi
    if [ -e "$tmp" ]; then
        printf 'cleanup.fixture_exists_after_cleanup=true\n' >> "$evidence"
        status=1
    else
        printf 'cleanup.fixture_exists_after_cleanup=false\n' >> "$evidence"
    fi
    exit "$status"
}
trap cleanup EXIT

: > "$evidence"
write_executable() {
    printf '%b\n' "$2" > "$bin/$1"
    chmod 755 "$bin/$1"
}

fail() {
    printf 'FAIL %s\n' "$1" >> "$evidence"
    exit 1
}

write_executable pacman '#!/bin/sh
case "$1:$2" in
  --color=never:-Sl) printf "core bash 5.3-1\\n" ;;
  -Qq:) printf "bash\\n" ;;
  --color=always:-Si|--color=always:-Qi) printf "Name : bash\\nVersion : 5.3-1\\n" ;;
  *) exit 64 ;;
esac'
write_executable paru '#!/bin/sh
printf "%s\\n" "$@" >> "$PACKTIDE_SMOKE_ROOT/paru.argv"
case "$1:$2" in
  --color=always:-Si) printf "Name : bash\\nVersion : 5.3-1\\n" ;;
  -S:*|-Rns:*) : > "$PACKTIDE_SMOKE_ROOT/transaction.invoked"; exit 99 ;;
  *) exit 0 ;;
esac'
write_executable fzf '#!/bin/sh
printf "%s\\n" "$@" > "$PACKTIDE_SMOKE_ROOT/fzf.args"
/bin/cat > "$PACKTIDE_SMOKE_ROOT/fzf.input"
exit 1'
write_executable sudo '#!/bin/sh
printf "%s\\n" "$@" > "$PACKTIDE_SMOKE_ROOT/sudo.invoked"
exit 99'
printf 'aur-tool\n' > "$cache/packtide/aur/packages"

path=$bin

if PACKTIDE_INSTALL_LIST_ONLY=1 PATH="$path" XDG_CACHE_HOME="$cache" PACKTIDE_UI_LANG=en "$binary" install > "$tmp/list.stdout" 2> "$tmp/list.stderr"; then
    list_status=0
else
    list_status=$?
fi
printf 'read.list.status=%s\n' "$list_status" >> "$evidence"
[ "$list_status" -eq 0 ] || fail 'read/list exited nonzero'
grep -q 'core' "$tmp/list.stdout" || fail 'read/list omitted pacman row'
grep -q 'aur-tool' "$tmp/list.stdout" || fail 'read/list omitted AUR row'
printf 'read.list.stdout=' >> "$evidence"
tr '\n' '|' < "$tmp/list.stdout" >> "$evidence"
printf '\n' >> "$evidence"

if PATH="$path" XDG_CACHE_HOME="$cache" PACKTIDE_UI_LANG=en "$binary" __preview install $'core\tbash\t5.3-1' > "$tmp/preview.stdout" 2> "$tmp/preview.stderr"; then
    preview_status=0
else
    preview_status=$?
fi
printf 'preview.status=%s\n' "$preview_status" >> "$evidence"
[ "$preview_status" -eq 0 ] || fail 'preview exited nonzero'
grep -q 'Name' "$tmp/preview.stdout" || fail 'preview omitted helper metadata'
printf 'preview.stdout=' >> "$evidence"
tr '\n' '|' < "$tmp/preview.stdout" >> "$evidence"
printf '\n' >> "$evidence"

if PATH="$path" XDG_CACHE_HOME="$cache" PACKTIDE_UI_LANG=en "$binary" install > "$tmp/cancel.stdout" 2> "$tmp/cancel.stderr"; then
    cancel_status=0
else
    cancel_status=$?
fi
printf 'cancel.status=%s\n' "$cancel_status" >> "$evidence"
[ "$cancel_status" -eq 0 ] || fail 'fzf cancellation was not successful'
grep -q 'No packages selected' "$tmp/cancel.stdout" || fail 'cancel output omitted cancellation result'
[ ! -e "$tmp/transaction.invoked" ] || fail 'cancel started a transaction'
printf 'cancel.stdout=' >> "$evidence"
tr '\n' '|' < "$tmp/cancel.stdout" >> "$evidence"
printf '\n' >> "$evidence"

mv "$bin/paru" "$bin/paru.hidden"
if PATH="$path" XDG_CACHE_HOME="$cache" PACKTIDE_UI_LANG=en "$binary" install > "$tmp/missing.stdout" 2> "$tmp/missing.stderr"; then
    missing_status=0
else
    missing_status=$?
fi
printf 'missing_helper.status=%s\n' "$missing_status" >> "$evidence"
[ "$missing_status" -ne 0 ] || fail 'missing helper unexpectedly succeeded'
grep -q "required AUR helper ('paru' or 'yay')" "$tmp/missing.stderr" || fail 'missing helper error omitted capability'
printf 'missing_helper.stderr=' >> "$evidence"
tr '\n' '|' < "$tmp/missing.stderr" >> "$evidence"
printf '\n' >> "$evidence"

[ ! -e "$tmp/sudo.invoked" ] || fail 'read/preview/cancel invoked sudo'
[ ! -e "$tmp/transaction.invoked" ] || fail 'read/preview/cancel invoked a transaction'
printf 'assertions.no_sudo=true\n' >> "$evidence"
printf 'assertions.no_transaction=true\n' >> "$evidence"
printf 'binary=%s\n' "$binary" >> "$evidence"
