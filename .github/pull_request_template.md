## Summary

<!-- What does this change and why? Link related issues (e.g. Closes #123). -->

## Type of change

- [ ] Bug fix
- [ ] New feature
- [ ] Refactor / chore
- [ ] Documentation
- [ ] Breaking change

## Security checklist

- [ ] No secrets cross the IPC boundary into the web UI
- [ ] No new network endpoints, or they are user-configured / documented in SECURITY.md
- [ ] New parsers have tests and a fuzz target

## Testing

<!-- How did you test this? Which platforms? -->

- [ ] `pnpm lint && pnpm typecheck && pnpm test`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`

## Screenshots

<!-- For UI changes: desktop + phone width, light + dark. -->
