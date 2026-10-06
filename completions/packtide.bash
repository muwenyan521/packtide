_packtide() {
    local cur prev commands
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    commands="check-updates install remove mirror-update downgrade sysup help"
    if [[ "$COMP_CWORD" == 1 && "$cur" != -* ]]; then
        mapfile -t COMPREPLY < <(compgen -W "$commands" -- "$cur")
        return
    fi
    case "$prev" in
        check-updates) mapfile -t COMPREPLY < <(compgen -W "--refresh" -- "$cur") ;;
        install) mapfile -t COMPREPLY < <(compgen -W "--refresh -y --no-ai" -- "$cur") ;;
        mirror-update) mapfile -t COMPREPLY < <(compgen -W "--country -c" -- "$cur") ;;
        sysup) mapfile -t COMPREPLY < <(compgen -W "--list -l --ui-lang --news-source --count" -- "$cur") ;;
        *) mapfile -t COMPREPLY < <(compgen -W "--help --version" -- "$cur") ;;
    esac
}
complete -F _packtide packtide
