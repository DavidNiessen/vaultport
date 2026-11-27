# Test SSH servers

Throw-away OpenSSH (Alpine) servers for the `integration` test suites of `ssh-engine` and
`sftp-engine`. **They use well-known passwords — never expose them beyond localhost.**

## Image (`sshd/`)

| What                      | Details                                                                                                                                                                                                            |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Users                     | `alice` / `alice-password` (password, keys, certificates, keyboard-interactive via PAM); `bob` / `bob-password` with `AuthenticationMethods publickey,password` (partial success)                                  |
| Host keys                 | fresh ed25519/ecdsa/rsa per container start, plus a per-container host CA (`/etc/ssh/host_ca.pub`) that signs the ed25519 host key (principals: `localhost,127.0.0.1,<hostname>`, override with `HOST_PRINCIPALS`) |
| User CA                   | `TrustedUserCAKeys /etc/ssh/user_ca.pub` (empty; tests write their own CA into it)                                                                                                                                 |
| Forwarding                | `AllowTcpForwarding yes`, `GatewayPorts clientspecified`, `AllowAgentForwarding yes`, `PermitOpen any`                                                                                                             |
| Environment               | `AcceptEnv LANG LC_* SSHC_*`                                                                                                                                                                                       |
| SFTP                      | `internal-sftp`                                                                                                                                                                                                    |
| Helpers (container-local) | `127.0.0.1:7777` TCP echo, `127.0.0.1:8080` tiny HTTP server (`/srv/www/index.txt`)                                                                                                                                |
| Lifetime                  | set `SSHD_LIFETIME=<seconds>` to make the container stop itself                                                                                                                                                    |

## Running the integration tests

```sh
docker build -t vaultport-test-sshd:1 docker/test-servers/sshd   # optional, tests build it if missing
cargo test -p ssh-engine -p sftp-engine --features integration
```

The tests start two containers (`a` with a published port, `b` reachable only through `a`)
on a private network and skip gracefully when docker is not available. Use
`SSH_ENGINE_TEST_IMAGE` to point them at a different image tag.

Behind a TLS-intercepting proxy, build with the proxy CA as a BuildKit secret:

```sh
docker build --network host --secret id=ca,src=/path/to/proxy-ca.crt \
  --build-arg https_proxy="$https_proxy" -t vaultport-test-sshd:1 docker/test-servers/sshd
```

## Manual playground

```sh
docker compose -f docker/test-servers/compose.yml up -d --build
ssh -p 2222 alice@127.0.0.1
ssh -J alice@127.0.0.1:2222 alice@sshd-b
docker compose -f docker/test-servers/compose.yml down
```
