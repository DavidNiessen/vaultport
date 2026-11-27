# SPDX-License-Identifier: GPL-3.0-or-later
# ssh-client shell integration for fish >= 3.1 (opt-in).
# Emits OSC 7 (cwd) and, on fish 3 (fish 4 marks prompts natively), OSC 133 A/B/C/D and
# OSC 633 E (command line). Safe to source twice.
if status is-interactive; and set -q SSH_CLIENT_SHELL_INTEGRATION; and not set -q __sshc_si
    set -g __sshc_si 1
    function __sshc_osc7
        printf '\e]7;file://%s%s\a' $hostname (string escape --style=url -- $PWD)
    end
    function __sshc_restore_status
        return $argv[1]
    end
    if not string match -qr '^[4-9]' -- $version
        function __sshc_preexec --on-event fish_preexec
            set -l c (string replace -a -- '\\' '\\\\' $argv[1] | string replace -a -- ';' '\\x3b')
            printf '\e]633;E;%s\a\e]133;C\a' (string join -- '\\x0a' $c)
        end
        function __sshc_postexec --on-event fish_postexec
            printf '\e]133;D;%s\a' $status
        end
    end
    if functions -q fish_prompt
        functions -c fish_prompt __sshc_orig_prompt
        function fish_prompt
            set -l ec $status
            __sshc_osc7
            if not string match -qr '^[4-9]' -- $version
                printf '\e]133;A\a'
            end
            __sshc_restore_status $ec
            __sshc_orig_prompt
            if not string match -qr '^[4-9]' -- $version
                printf '\e]133;B\a'
            end
        end
    end
end
