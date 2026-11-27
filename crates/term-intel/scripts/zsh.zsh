# SPDX-License-Identifier: GPL-3.0-or-later
# Vaultport shell integration for zsh >= 5.1 (opt-in).
# Emits OSC 7 (cwd), OSC 133 A/B/C/D and OSC 633 E (command line). Safe to source twice.
if [[ -o interactive && -n ${VAULTPORT_SHELL_INTEGRATION-} && -z ${__sshc_si-} ]]; then
  typeset -g __sshc_si=1 __sshc_ran=
  __sshc_osc7() {
    emulate -L zsh
    setopt no_multibyte
    local s=$PWD out= c
    local -i i
    for (( i = 1; i <= $#s; i++ )); do
      c=$s[i]
      case $c in
        ([-/._~A-Za-z0-9]) out+=$c ;;
        (*) out+=%${(l:2::0:)$(( [##16] #c ))} ;;
      esac
    done
    printf '\e]7;file://%s%s\a' "$HOST" "$out"
  }
  __sshc_precmd() {
    local ec=$?
    [[ -n $__sshc_ran ]] && printf '\e]133;D;%s\a' "$ec"
    __sshc_ran=
    __sshc_osc7
    printf '\e]133;A\a'
  }
  __sshc_mark_b() {
    [[ $PS1 == *$'\e]133;B'* ]] || PS1+=$'%{\e]133;B\a%}'
  }
  __sshc_preexec() {
    emulate -L zsh
    __sshc_ran=1
    local c=${1//\\/\\\\}
    c=${c//;/\\x3b}
    c=${c//$'\n'/\\x0a}
    c=${c//$'\a'/\\x07}
    c=${c//$'\e'/\\x1b}
    printf '\e]633;E;%s\a\e]133;C\a' "$c"
  }
  typeset -ga precmd_functions preexec_functions
  # Capture $? first, add the B mark last (after themes that rebuild PS1).
  precmd_functions=(__sshc_precmd ${precmd_functions:#__sshc_*} __sshc_mark_b)
  preexec_functions+=(__sshc_preexec)
fi
