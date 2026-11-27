# SPDX-License-Identifier: GPL-3.0-or-later
# Vaultport shell integration for bash >= 4.4 (opt-in).
# Emits OSC 7 (cwd), OSC 133 A/B/C/D (prompt / command marks) and OSC 633;E (command line).
# Safe to source twice.
if [[ $- == *i* && -n ${VAULTPORT_SHELL_INTEGRATION-} && -z ${__sshc_si-} ]]; then
  __sshc_si=1
  __sshc_first=1
  __sshc_osc7() {
    local LC_ALL=C s=$PWD out= c i
    for ((i = 0; i < ${#s}; i++)); do
      c=${s:i:1}
      case $c in
        [-/._~A-Za-z0-9]) out+=$c ;;
        *) printf -v c '%%%02X' "'$c"; out+=$c ;;
      esac
    done
    printf '\e]7;file://%s%s\a' "${HOSTNAME-}" "$out"
  }
  # Number of the newest history entry, so __sshc_preexec can tell a new command from one
  # bash did not record (HISTCONTROL=ignorespace, set +o history).
  __sshc_histno() {
    local h
    h=$(HISTTIMEFORMAT= builtin history 1)
    [[ $h =~ ^[[:space:]]*([0-9]+) ]] && printf '%s' "${BASH_REMATCH[1]}"
  }
  # Runs from PS0 in a subshell, after a command is read and before it runs.
  __sshc_preexec() {
    local h n c
    h=$(HISTTIMEFORMAT= builtin history 1)
    [[ $h =~ ^[[:space:]]*([0-9]+)[*]?[[:space:]]+(.*)$ ]] || return 0
    n=${BASH_REMATCH[1]} c=${BASH_REMATCH[2]}
    [[ $n == "${__sshc_hn-}" ]] && return 0
    c=${c//\\/\\\\}
    c=${c//;/\\x3b}
    c=${c//$'\n'/\\x0a}
    c=${c//$'\a'/\\x07}
    c=${c//$'\e'/\\x1b}
    printf '\e]633;E;%s\a' "$c"
  }
  __sshc_prompt() {
    local ec=$?
    __sshc_hn=$(__sshc_histno)
    [[ -z $__sshc_first ]] && printf '\e]133;D;%s\a' "$ec"
    __sshc_first=
    __sshc_osc7
    printf '\e]133;A\a'
    [[ $PS1 == *'\e]133;B'* ]] || PS1+='\[\e]133;B\a\]'
    return "$ec"
  }
  PROMPT_COMMAND="__sshc_prompt${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
  PS0='$(__sshc_preexec)\e]133;C\a'"${PS0-}"
fi
