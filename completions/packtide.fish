complete -c packtide -f -n '__fish_use_subcommand' -a 'check-updates install remove mirror-update downgrade sysup help'
complete -c packtide -l refresh -s y -n '__fish_seen_subcommand_from install check-updates'
complete -c packtide -l no-ai -n '__fish_seen_subcommand_from install'
complete -c packtide -l country -s c -n '__fish_seen_subcommand_from mirror-update'
complete -c packtide -l list -s l -l ui-lang -l news-source -l count -n '__fish_seen_subcommand_from sysup'
complete -c packtide -l help -l version
