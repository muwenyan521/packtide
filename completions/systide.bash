_systide() {
    local cur="${COMP_WORDS[COMP_CWORD]}"
    mapfile -t COMPREPLY < <(compgen -W "--list -l --ui-lang --news-source --count --help --version" -- "$cur")
}
complete -F _systide systide
