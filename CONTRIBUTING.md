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

# The native Windows on ARM build (from Linux, with llvm-mingw on PATH)
rustup target add aarch64-pc-windows-gnullvm
cargo clippy --locked --target aarch64-pc-windows-gnullvm --all-targets -- -D warnings
cargo build --locked --release --target aarch64-pc-windows-gnullvm
```

On Windows, `scripts/build-release.ps1` runs the tests, lint and MSVC build and
compiles the installer; add `-Arch arm64` on a Windows on ARM PC for the native ARM build and its installer. CI runs it on every push, with a RustSec audit of
`Cargo.lock`. Releases are built by GitHub Actions: see
[docs/RELEASING.md](docs/RELEASING.md) and the [changelog](CHANGELOG.md).

## Rules

- **Every check explains itself** in [`src/explain/`](src/explain/).
- **Every string in six languages** in [`src/i18n.rs`](src/i18n.rs), or in
  [`i18n-pending/`](i18n-pending/) if you can't translate it.
- **Every change is undoable,** or says it is one-way before the user agrees.
- **Test on a real Windows PC** and say in the pull request what you tried.

Maintainers and coding agents: the full working rules are in
[AGENTS.md](AGENTS.md).
