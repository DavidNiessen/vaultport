#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Generates fresh host keys, a host CA and a host certificate, starts two helper services
# reachable only from inside the container, then runs sshd in the foreground.
#
#   127.0.0.1:7777  TCP echo service (socat)
#   127.0.0.1:8080  minimal HTTP/1.0 server answering every request with /srv/www/index.txt
#
# Environment:
#   SSHD_LIFETIME     seconds after which the container stops itself (default: unlimited)
#   HOST_PRINCIPALS   principals of the host certificate (default: localhost,127.0.0.1,<hostname>)
set -eu

ssh-keygen -q -A
ssh-keygen -q -t ed25519 -N '' -C test-host-ca -f /etc/ssh/host_ca
principals="${HOST_PRINCIPALS:-localhost,127.0.0.1,$(hostname)}"
ssh-keygen -q -s /etc/ssh/host_ca -I "$(hostname)-host" -h -n "$principals" -V -5m:+1d \
    /etc/ssh/ssh_host_ed25519_key.pub
touch /etc/ssh/user_ca.pub

socat TCP-LISTEN:7777,bind=127.0.0.1,fork,reuseaddr EXEC:cat &
socat TCP-LISTEN:8080,bind=127.0.0.1,fork,reuseaddr EXEC:/usr/local/bin/http-once.sh &

if [ -n "${SSHD_LIFETIME:-}" ]; then
    ( sleep "$SSHD_LIFETIME"; kill -TERM 1 ) &
fi

exec /usr/sbin/sshd.pam -D -e -f /etc/ssh/sshd_config
