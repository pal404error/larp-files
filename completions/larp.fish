# fish completion for larp. Install to:
#   cp completions/larp.fish ~/.config/fish/completions/
complete -c larp -f
complete -c larp -n '__fish_use_subcommand' -a 'append list dump search recall compact import export stats mcp'
complete -c larp -n '__fish_seen_subcommand_from append' -l type -a 'fact preference episode correction' -d 'Record type'
complete -c larp -n '__fish_seen_subcommand_from append' -l salience -d 'Salience 0..1'
complete -c larp -n '__fish_seen_subcommand_from append' -l ttl -d 'TTL in seconds (0 = never)'
complete -c larp -n '__fish_seen_subcommand_from append' -l supersedes -d 'ID this record supersedes'
complete -c larp -n '__fish_seen_subcommand_from recall' -l max-tokens -d 'Token budget'
complete -c larp -n '__fish_seen_subcommand_from export' -l live -d 'Live records only'
complete -c larp -l file -r -d 'Memory file'
complete -c larp -l version -d 'Show version'
complete -c larp -s V -d 'Show version'
