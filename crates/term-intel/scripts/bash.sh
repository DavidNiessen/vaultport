# SPDX-License-Identifier: GPL-3.0-or-later
# ssh-client shell integration for bash >= 4.4 (opt-in).
# Emits OSC 7 (cwd) and OSC 133 A/B/C/D (prompt / command marks). Safe to source twice.
if [[ $- == *i* && -n ${SSH_CLIENT_SHELL_INTEGRATION-} && -z ${__sshc_si-} ]]; then
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
  __sshc_prompt() {
    local ec=$?
    [[ -z $__sshc_first ]] && printf '\e]133;D;%s\a' "$ec"
    __sshc_first=
    __sshc_osc7
    printf '\e]133;A\a'
    [[ $PS1 == *'\e]133;B'* ]] || PS1+='\[\e]133;B\a\]'
    return "$ec"
  }
  PROMPT_COMMAND="__sshc_prompt${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
  PS0='\e]133;C\a'"${PS0-}"
fi
