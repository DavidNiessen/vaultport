# SPDX-License-Identifier: GPL-3.0-or-later
#
# Throw-away OpenSSH server used by the `integration` test suites of `ssh-engine` and
# `sftp-engine`. NEVER expose it to a network you do not control: it has well-known passwords.
#
# Build:  docker build -t vaultport-test-sshd:1 docker/test-servers/sshd
# Behind a TLS-intercepting proxy, pass its CA as a build secret:
#         docker build --secret id=ca,src=/path/to/ca.crt --network host \
#           --build-arg https_proxy=$https_proxy -t vaultport-test-sshd:1 docker/test-servers/sshd
ARG ALPINE_IMAGE=alpine:3.22
FROM ${ALPINE_IMAGE}

RUN --mount=type=secret,id=ca,required=false \
    if [ -s /run/secrets/ca ]; then cp /run/secrets/ca /etc/ssl/certs/ca-certificates.crt; fi \
    && apk add --no-cache openssh-server-pam openssh-sftp-server openssh-keygen openssh-client socat \
    && rm -f /etc/ssh/ssh_host_*

# alice: password + keys + certificates + keyboard-interactive (via PAM).
# bob:   needs BOTH a public key and a password (AuthenticationMethods publickey,password).
RUN adduser -D -s /bin/sh alice \
    && adduser -D -s /bin/sh bob \
    && echo 'alice:alice-password' | chpasswd \
    && echo 'bob:bob-password' | chpasswd \
    && mkdir -p /home/alice/.ssh /home/bob/.ssh /srv/www \
    && touch /home/alice/.ssh/authorized_keys /home/bob/.ssh/authorized_keys \
    && chown -R alice:alice /home/alice && chown -R bob:bob /home/bob \
    && chmod 700 /home/alice/.ssh /home/bob/.ssh \
    && chmod 600 /home/alice/.ssh/authorized_keys /home/bob/.ssh/authorized_keys \
    && printf 'hello from the test http server\n' > /srv/www/index.txt

COPY sshd_config /etc/ssh/sshd_config
COPY entrypoint.sh http-once.sh /usr/local/bin/
RUN chmod 755 /usr/local/bin/entrypoint.sh /usr/local/bin/http-once.sh

EXPOSE 22
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
