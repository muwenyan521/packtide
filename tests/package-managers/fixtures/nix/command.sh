#!/bin/sh
printf 'arg=<%s>\n' "$@"
printf 'locale=<%s>\n' "$LC_ALL"
case "${0##*/}" in
    disabled-nix-command)
        printf "error: experimental Nix feature 'nix-command' is disabled; add '--extra-experimental-features nix-command' to enable it\n" >&2
        exit 1
        ;;
    disabled-flakes)
        printf "error: experimental Nix feature 'flakes' is disabled; add '--extra-experimental-features flakes' to enable it\n" >&2
        exit 1
        ;;
    unrelated-failure)
        printf 'experimental package build failed\n' >&2
        exit 7
        ;;
    successful-warning)
        printf "warning: experimental Nix feature 'flakes' is disabled elsewhere\n" >&2
        ;;
esac
