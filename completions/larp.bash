# bash completion for larp. Install to a bash-completion directory, e.g.:
#   cp completions/larp.bash ~/.local/share/bash-completion/completions/larp
# then restart the shell.
_larp() {
  local cur prev cmds
  COMPREPLY=()
  cur="${COMP_WORDS[COMP_CWORD]}"
  prev="${COMP_WORDS[COMP_CWORD-1]}"
  cmds="append list dump search recall compact import export stats mcp"
  case "$prev" in
    --file) COMPREPLY=( $(compgen -f -- "$cur") ); return 0 ;;
    --type) COMPREPLY=( $(compgen -W "fact preference episode correction" -- "$cur") ); return 0 ;;
    import) COMPREPLY=( $(compgen -f -- "$cur") ); return 0 ;;
    larp) COMPREPLY=( $(compgen -W "$cmds --file --version -V" -- "$cur") ); return 0 ;;
    append) COMPREPLY=( $(compgen -W "--type --salience --ttl --supersedes --file" -- "$cur") ); return 0 ;;
    recall) COMPREPLY=( $(compgen -W "--max-tokens --file" -- "$cur") ); return 0 ;;
    export) COMPREPLY=( $(compgen -W "--live --file" -- "$cur") ); return 0 ;;
    list|dump|search|compact|stats|mcp) COMPREPLY=( $(compgen -W "--file" -- "$cur") ); return 0 ;;
  esac
  return 0
}
complete -F _larp larp
