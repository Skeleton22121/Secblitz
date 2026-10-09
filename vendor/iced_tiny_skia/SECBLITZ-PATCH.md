# Secblitz patch

This folder is a copy of `iced_tiny_skia` 0.14.1, the CPU renderer of the iced
toolkit (https://github.com/iced-rs/iced), as published on crates.io. The root
`Cargo.toml` points to it with a `[patch.crates-io]` entry.

## What is different

One change, in `src/window/compositor.rs`, marked with a "Secblitz patch"
comment. Upstream compares each frame with the previous one and repaints only
the parts it thinks changed. That comparison misses some changes (text and
drawings updated in place, moved and clipped content), which leaves stale
pixels on screen. The patch makes every redraw repaint the whole window. iced
only redraws when something asked for it, so an idle window costs nothing.

Everything else is unchanged from upstream.

## License

Upstream's MIT license is kept, and `Cargo.toml` still names it. Upstream's copyright notice and license text are in [LICENSE](LICENSE). The Secblitz
change is released under the same terms.
