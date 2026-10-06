set -eu
state=${PACKTIDE_QUERY_STATE:?}
lock="$state.lock"
locked=0
output=
child=
cleanup() {
    status=$?
    trap - TERM INT HUP EXIT
    if [ -n "$child" ]; then
        kill -TERM -- "-$child" 2>/dev/null || kill "$child" 2>/dev/null || true
        wait "$child" 2>/dev/null || true
    fi
    if [ "$locked" = 1 ]; then rmdir "$lock"; fi
    if [ -n "$output" ]; then rm -f "$output"; fi
    exit "$status"
}
trap cleanup EXIT
trap 'exit 143' TERM HUP
trap 'exit 130' INT
while ! mkdir "$lock" 2>/dev/null; do
    if [ ! -f "$state" ]; then exit 0; fi
    sleep 0.01
done
locked=1
if [ ! -f "$state" ]; then exit 0; fi
generation=$(cat "$state")
generation=$((generation + 1))
printf '%s\n' "$generation" >"$state"
rmdir "$lock"
locked=0
sleep 0.3
current=$(cat "$state" 2>/dev/null || true)
if [ "$current" != "$generation" ]; then exit 0; fi
q=$(printf '%s' "$1" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')
refresh=$2
output=$(mktemp "${TMPDIR:-/tmp}/packtide-query.XXXXXX")
set -- install
if [ "$refresh" = refresh ]; then set -- "$@" --refresh; fi
if [ "${#q}" -ge 2 ]; then set -- "$@" "$q"; fi
setsid env PACKTIDE_INSTALL_LIST_ONLY=1 "$0" "$@" >"$output" &
child=$!
while kill -0 "$child" 2>/dev/null; do
    current=$(cat "$state" 2>/dev/null || true)
    if [ "$current" != "$generation" ]; then exit 0; fi
    sleep 0.05
done
status=0
wait "$child" || status=$?
child=
if [ "$status" != 0 ]; then exit "$status"; fi
current=$(cat "$state" 2>/dev/null || true)
if [ "$current" = "$generation" ]; then cat "$output"; fi
