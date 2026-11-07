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

# Directory changes and a terminal's initial folder sync environments even without a command.
# A prompt retry also covers a visit made while Dev Console was stopped or the vault locked.
_dev_console_project_visit() {
  [[ -o interactive ]] || return 0
  (command python3 -I __DEV_CONSOLE_CLIENT__ "$PWD" --visit </dev/null >/dev/null 2>&1) &!
  return 0
}
add-zsh-hook -d chpwd _dev_console_project_visit 2>/dev/null || true
add-zsh-hook -d precmd _dev_console_project_visit 2>/dev/null || true
add-zsh-hook chpwd _dev_console_project_visit
add-zsh-hook precmd _dev_console_project_visit
_dev_console_project_visit
