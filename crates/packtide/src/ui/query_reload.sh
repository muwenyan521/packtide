set -eu
state=${PACKTIDE_QUERY_STATE:?}
output=
child=
cleanup() {
    status=$?
    trap - TERM INT HUP EXIT
    if [ -n "$child" ]; then
        trap '' TERM INT HUP
        kill -TERM -- "-$$" 2>/dev/null || kill "$child" 2>/dev/null || true
        wait "$child" 2>/dev/null || true
    fi
    if [ -n "$output" ]; then rm -f "$output"; fi
    exit "$status"
}
trap cleanup EXIT
trap 'exit 143' TERM HUP
trap 'exit 130' INT
if [ ! -f "$state" ]; then exit 0; fi
generation=$$
printf '%s\n' "$generation" >"$state"
sleep 0.3
current=$(cat "$state" 2>/dev/null || true)
if [ "$current" != "$generation" ]; then exit 0; fi
q=$(printf '%s' "$1" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')
refresh=$2
output=$(mktemp "${TMPDIR:-/tmp}/packtide-query.XXXXXX")
set -- install
if [ "$refresh" = refresh ]; then set -- "$@" --refresh; fi
q_length=$(printf '%s' "$q" | awk '{ print length($0) }')
if [ "$q_length" -ge 2 ]; then set -- "$@" "$q"; fi
env PACKTIDE_INSTALL_LIST_ONLY=1 "$0" "$@" >"$output" &
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
