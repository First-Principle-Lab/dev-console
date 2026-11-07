# Capture the command's actual working directory before execution. No history-file inference.
# The client sends stdin over the private socket; only filtered commands are stored by the app.
_dev_console_project_history() {
  [[ -o interactive && -n "$1" && "$1" != [[:space:]]* ]] || return 0
  (builtin print -rn -- "$1" | command python3 -I __DEV_CONSOLE_CLIENT__ "$PWD" >/dev/null 2>&1) &!
  return 0
}
autoload -Uz add-zsh-hook
add-zsh-hook -d zshaddhistory _dev_console_project_history 2>/dev/null || true
add-zsh-hook zshaddhistory _dev_console_project_history
