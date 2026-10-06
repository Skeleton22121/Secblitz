# Contributing

Bug reports, translations and fixes are welcome. For larger changes, open an
issue first. Report security problems privately, as described in
[SECURITY.md](SECURITY.md).

## Build and test

```sh
# Portable logic and tests, on any OS
cargo test --locked --all-targets

# The Windows app (from Linux, with a MinGW-w64 toolchain)
cargo build --locked --release --target x86_64-pc-windows-gnu
cargo clippy --locked --target x86_64-pc-windows-gnu --all-targets -- -D warnings
```

On Windows, `scripts/build-release.ps1` runs the tests, lint and MSVC build and
compiles the installer. CI runs it on every push, with a RustSec audit of
`Cargo.lock`. Releases are built by GitHub Actions: see
[docs/RELEASING.md](docs/RELEASING.md) and the [changelog](CHANGELOG.md).

## Rules

- **No jargon** in anything a user reads. No acronyms, no fear, no em dashes.
- **Every check explains itself** in [`src/explain/`](src/explain/).
- **Every string in six languages** in [`src/i18n.rs`](src/i18n.rs), or in
  [`i18n-pending/`](i18n-pending/) if you can't translate it.
- **Every change is undoable,** or says it is one-way before the user agrees.
- **Test on a real Windows PC** and say in the pull request what you tried.

Maintainers and coding agents: the full working rules are in
[AGENTS.md](AGENTS.md).
