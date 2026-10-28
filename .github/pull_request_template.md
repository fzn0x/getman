## What and why

<!-- What this changes, and why. For a large change, link the issue where it was agreed. -->

## Checks

- [ ] `cargo fmt --all`, `cargo clippy --workspace --all-targets -- --deny warnings` and `cargo test --workspace` pass
- [ ] For the browser UI: `npm run check` and `npm test` pass in `web/`
- [ ] Changes to the YAML format write nothing at the default and still read older files
- [ ] Added or updated tests for what changed
- [ ] Updated the page in `docs/` (and `docs/cli.md` for commands and flags) and `CHANGELOG.md` under Unreleased
- [ ] Commits follow [Conventional Commits](https://www.conventionalcommits.org), one logical change each
