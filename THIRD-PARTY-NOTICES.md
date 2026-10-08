# Third-party notices

Secblitz is released under the [MIT license](LICENSE). It includes or uses the
work of others, listed here with the terms each one comes under.

## Fonts

Lexend (Regular, Medium, SemiBold and Bold) is bundled in the app from
`assets/fonts`. Copyright 2018 The Lexend Project Authors
(https://github.com/googlefonts/lexend), with Reserved Font Name "RevReading
Lexend". It is licensed under the SIL Open Font License 1.1. The full text is in
`assets/fonts/Lexend-LICENSE.txt`.

IBM Plex Sans is used by the website, from `assets/fonts`. Copyright 2017 IBM
Corp., with Reserved Font Name "Plex". It is licensed under the SIL Open Font
License 1.1. The full text is in `assets/fonts/IBMPlexSans-LICENSE.txt`.

## Icons

The interface icons are Fluent UI System Icons by Microsoft
(https://github.com/microsoft/fluentui-system-icons), 24px Regular and Filled
variants, used under the MIT license. The notice and license text are in
`assets/ICONS-LICENSE.txt`.

## Patched iced_tiny_skia

`vendor/iced_tiny_skia` is a copy of the iced CPU renderer (iced_tiny_skia
0.14.1) from https://github.com/iced-rs/iced with one small change, described
in `vendor/iced_tiny_skia/SECBLITZ-PATCH.md`. It stays under upstream's MIT
license.

## Block lists (downloaded, not shipped)

Web protection downloads its block lists while the program runs. No list text
is included in Secblitz or its installer. The lists come from these projects,
and each stays under its own license:

| Project | Where Secblitz downloads it from | License |
| --- | --- | --- |
| EasyList and EasyPrivacy | https://easylist.to | GPLv3 or CC BY-SA 3.0 (dual licensed) |
| AdGuard filters | https://filters.adtidy.org | GPLv3 |
| AdGuard HostlistsRegistry, including the HaGeZi lists | https://adguardteam.github.io/HostlistsRegistry | GPLv3 |

The exact addresses are in `src/filter/lists.rs`. Secblitz reads the lists to
decide which sites to block and does not redistribute them.

## Rust crates

The program is built from the Rust crates below. Each is used under the terms
shown, and where a crate offers a choice (for example "MIT OR Apache-2.0"),
Secblitz uses it under the permissive option. The licenses in use are:
MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, BSL-1.0, Unicode-3.0,
0BSD, Unlicense, CC0-1.0 and CDLA-Permissive-2.0. None of them is a copyleft
license that applies to Secblitz.

The exact versions are pinned in `Cargo.lock`. The table lists the crates
compiled into the Windows program (build tools that only run on the developer's
computer are left out). It can be regenerated with:

```sh
cargo metadata --locked --offline --format-version 1 --filter-platform x86_64-pc-windows-msvc
```

The `deny.toml` file and the CI "licenses" check keep this list honest: a new
dependency under any other license fails the build.

| Crate | Version | License |
| --- | --- | --- |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 |
| anstream | 1.0.0 | MIT OR Apache-2.0 |
| anstyle | 1.0.14 | MIT OR Apache-2.0 |
| anstyle-parse | 1.0.0 | MIT OR Apache-2.0 |
| anstyle-query | 1.1.5 | MIT OR Apache-2.0 |
| anstyle-wincon | 3.0.11 | MIT OR Apache-2.0 |
| anyhow | 1.0.104 | MIT OR Apache-2.0 |
| arrayref | 0.3.9 | BSD-2-Clause |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 |
| ash | 0.38.0+1.3.281 | MIT OR Apache-2.0 |
| async-compression | 0.4.50 | MIT OR Apache-2.0 |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT |
| base64 | 0.22.1 | MIT OR Apache-2.0 |
| base64 | 0.23.1 | MIT OR Apache-2.0 |
| base64ct | 1.8.3 | Apache-2.0 OR MIT |
| bit-set | 0.8.0 | Apache-2.0 OR MIT |
| bit-vec | 0.8.0 | Apache-2.0 OR MIT |
| bitflags | 1.3.2 | MIT/Apache-2.0 |
| bitflags | 2.13.2 | MIT OR Apache-2.0 |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT |
| bytemuck_derive | 1.12.1 | Zlib OR Apache-2.0 OR MIT |
| byteorder-lite | 0.1.0 | Unlicense OR MIT |
| bytes | 1.12.1 | MIT |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 |
| chacha20 | 0.10.2 | MIT OR Apache-2.0 |
| clap | 4.6.7 | MIT OR Apache-2.0 |
| clap_builder | 4.6.7 | MIT OR Apache-2.0 |
| clap_derive | 4.6.7 | MIT OR Apache-2.0 |
| clap_lex | 1.1.1 | MIT OR Apache-2.0 |
| clipboard-win | 5.4.1 | BSL-1.0 |
| codespan-reporting | 0.12.0 | Apache-2.0 |
| color_quant | 1.1.0 | MIT |
| colorchoice | 1.0.5 | MIT OR Apache-2.0 |
| compression-codecs | 0.4.45 | MIT OR Apache-2.0 |
| compression-core | 0.4.33 | MIT OR Apache-2.0 |
| const-oid | 0.9.6 | Apache-2.0 OR MIT |
| core_maths | 0.1.1 | MIT |
| cosmic-text | 0.15.0 | MIT OR Apache-2.0 |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 |
| cpufeatures | 0.3.1 | MIT OR Apache-2.0 |
| crc32fast | 1.5.2 | MIT OR Apache-2.0 |
| cryoglyph | 0.1.0 | MIT OR Apache-2.0 OR Zlib |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 |
| cursor-icon | 1.2.0 | MIT OR Apache-2.0 OR Zlib |
| curve25519-dalek | 4.1.3 | BSD-3-Clause |
| curve25519-dalek-derive | 0.1.1 | MIT/Apache-2.0 |
| data-url | 0.3.2 | MIT OR Apache-2.0 |
| der | 0.7.10 | Apache-2.0 OR MIT |
| digest | 0.10.7 | MIT OR Apache-2.0 |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 |
| document-features | 0.2.12 | MIT OR Apache-2.0 |
| dpi | 0.1.2 | Apache-2.0 AND MIT |
| ed25519 | 2.2.3 | Apache-2.0 OR MIT |
| ed25519-dalek | 2.2.0 | BSD-3-Clause |
| equivalent | 1.0.2 | Apache-2.0 OR MIT |
| error-code | 3.4.0 | BSL-1.0 |
| etagere | 0.2.15 | MIT/Apache-2.0 |
| euclid | 0.22.14 | MIT OR Apache-2.0 |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 |
| flate2 | 1.1.10 | MIT OR Apache-2.0 |
| float-cmp | 0.9.0 | MIT |
| float_next_after | 1.0.0 | MIT |
| foldhash | 0.1.5 | Zlib |
| foldhash | 0.2.0 | Zlib |
| font-types | 0.10.1 | MIT OR Apache-2.0 |
| font-types | 0.12.6 | MIT OR Apache-2.0 |
| fontdb | 0.23.0 | MIT |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 |
| fs2 | 0.4.3 | MIT/Apache-2.0 |
| futures | 0.3.34 | MIT OR Apache-2.0 |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 |
| futures-core | 0.3.34 | MIT OR Apache-2.0 |
| futures-executor | 0.3.34 | MIT OR Apache-2.0 |
| futures-io | 0.3.34 | MIT OR Apache-2.0 |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 |
| futures-task | 0.3.34 | MIT OR Apache-2.0 |
| futures-util | 0.3.34 | MIT OR Apache-2.0 |
| generic-array | 0.14.7 | MIT |
| getrandom | 0.2.17 | MIT OR Apache-2.0 |
| getrandom | 0.4.3 | MIT OR Apache-2.0 |
| gif | 0.13.3 | MIT OR Apache-2.0 |
| glam | 0.25.0 | MIT OR Apache-2.0 |
| glow | 0.16.0 | MIT OR Apache-2.0 OR Zlib |
| glutin_wgl_sys | 0.6.1 | Apache-2.0 |
| gpu-alloc | 0.6.2 | MIT OR Apache-2.0 |
| gpu-alloc-types | 0.3.1 | MIT OR Apache-2.0 |
| gpu-allocator | 0.27.0 | MIT OR Apache-2.0 |
| gpu-descriptor | 0.3.2 | MIT OR Apache-2.0 |
| gpu-descriptor-types | 0.2.0 | MIT OR Apache-2.0 |
| guillotiere | 0.6.2 | MIT/Apache-2.0 |
| half | 2.7.1 | MIT OR Apache-2.0 |
| harfrust | 0.3.2 | MIT |
| hashbrown | 0.15.5 | MIT OR Apache-2.0 |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 |
| heck | 0.5.0 | MIT OR Apache-2.0 |
| hex | 0.4.3 | MIT OR Apache-2.0 |
| hexf-parse | 0.2.1 | CC0-1.0 |
| http | 1.5.0 | MIT OR Apache-2.0 |
| http-body | 1.1.0 | MIT |
| http-body-util | 0.1.5 | MIT |
| httparse | 1.10.1 | MIT OR Apache-2.0 |
| hyper | 1.11.1 | MIT |
| hyper-rustls | 0.27.10 | Apache-2.0 OR ISC OR MIT |
| hyper-util | 0.1.21 | MIT |
| iced | 0.14.0 | MIT |
| iced_core | 0.14.0 | MIT |
| iced_debug | 0.14.0 | MIT |
| iced_futures | 0.14.0 | MIT |
| iced_graphics | 0.14.0 | MIT |
| iced_program | 0.14.0 | MIT |
| iced_renderer | 0.14.0 | MIT |
| iced_runtime | 0.14.0 | MIT |
| iced_tiny_skia | 0.14.1 | MIT |
| iced_wgpu | 0.14.0 | MIT |
| iced_widget | 0.14.2 | MIT |
| iced_winit | 0.14.1 | MIT |
| icu_collections | 2.3.0 | Unicode-3.0 |
| icu_locale_core | 2.3.0 | Unicode-3.0 |
| icu_normalizer | 2.3.0 | Unicode-3.0 |
| icu_normalizer_data | 2.3.0 | Unicode-3.0 |
| icu_properties | 2.3.0 | Unicode-3.0 |
| icu_properties_data | 2.3.0 | Unicode-3.0 |
| icu_provider | 2.3.1 | Unicode-3.0 |
| idna | 1.1.0 | MIT OR Apache-2.0 |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT |
| image | 0.25.10 | MIT OR Apache-2.0 |
| image-webp | 0.2.4 | MIT OR Apache-2.0 |
| imagesize | 0.13.0 | MIT |
| indexmap | 2.14.2 | Apache-2.0 OR MIT |
| ipnet | 2.12.2 | MIT OR Apache-2.0 |
| is_terminal_polyfill | 1.70.2 | MIT OR Apache-2.0 |
| itoa | 1.0.18 | MIT OR Apache-2.0 |
| kamadak-exif | 0.6.1 | BSD-2-Clause |
| khronos-egl | 6.0.0 | MIT/Apache-2.0 |
| kurbo | 0.10.4 | MIT OR Apache-2.0 |
| kurbo | 0.11.3 | Apache-2.0 OR MIT |
| libc | 0.2.189 | MIT OR Apache-2.0 |
| libloading | 0.8.9 | ISC |
| libm | 0.2.16 | MIT |
| lilt | 0.8.2 | MIT |
| linebender_resource_handle | 0.1.1 | Apache-2.0 OR MIT |
| litemap | 0.8.3 | Unicode-3.0 |
| litrs | 1.0.0 | MIT OR Apache-2.0 |
| lock_api | 0.4.14 | MIT OR Apache-2.0 |
| log | 0.4.34 | MIT OR Apache-2.0 |
| lru | 0.16.4 | MIT |
| lru-slab | 0.1.3 | MIT OR Apache-2.0 OR Zlib |
| lyon | 1.0.19 | MIT OR Apache-2.0 |
| lyon_algorithms | 1.0.21 | MIT OR Apache-2.0 |
| lyon_geom | 1.0.19 | MIT OR Apache-2.0 |
| lyon_path | 1.0.19 | MIT OR Apache-2.0 |
| lyon_tessellation | 1.0.22 | MIT OR Apache-2.0 |
| memchr | 2.8.3 | Unlicense OR MIT |
| memmap2 | 0.9.11 | MIT OR Apache-2.0 |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 |
| mio | 1.2.3 | MIT |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 |
| mutate_once | 0.1.2 | BSD-2-Clause |
| naga | 27.0.3 | MIT OR Apache-2.0 |
| num-traits | 0.2.19 | MIT OR Apache-2.0 |
| once_cell | 1.21.4 | MIT OR Apache-2.0 |
| once_cell_polyfill | 1.70.2 | MIT OR Apache-2.0 |
| ordered-float | 5.5.0 | MIT |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 |
| pico-args | 0.5.0 | MIT |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT |
| pkcs8 | 0.10.2 | Apache-2.0 OR MIT |
| png | 0.17.16 | MIT OR Apache-2.0 |
| potential_utf | 0.1.6 | Unicode-3.0 |
| ppv-lite86 | 0.2.21 | MIT OR Apache-2.0 |
| presser | 0.3.1 | MIT OR Apache-2.0 |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 |
| profiling | 1.0.18 | MIT OR Apache-2.0 |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 |
| quick-error | 2.0.1 | MIT/Apache-2.0 |
| quinn | 0.11.12 | MIT OR Apache-2.0 |
| quinn-proto | 0.11.19 | MIT OR Apache-2.0 |
| quinn-udp | 0.5.16 | MIT OR Apache-2.0 |
| quote | 1.0.47 | MIT OR Apache-2.0 |
| rand | 0.10.3 | MIT OR Apache-2.0 |
| rand | 0.8.8 | MIT OR Apache-2.0 |
| rand_chacha | 0.3.1 | MIT OR Apache-2.0 |
| rand_core | 0.10.1 | MIT OR Apache-2.0 |
| rand_core | 0.6.4 | MIT OR Apache-2.0 |
| rand_pcg | 0.10.2 | MIT OR Apache-2.0 |
| range-alloc | 0.1.5 | MIT OR Apache-2.0 |
| rangemap | 1.8.0 | MIT/Apache-2.0 |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib |
| read-fonts | 0.35.0 | MIT OR Apache-2.0 |
| read-fonts | 0.41.0 | MIT OR Apache-2.0 |
| renderdoc-sys | 1.1.0 | MIT OR Apache-2.0 |
| reqwest | 0.12.28 | MIT OR Apache-2.0 |
| resvg | 0.45.1 | Apache-2.0 OR MIT |
| rgb | 0.8.53 | MIT |
| ring | 0.17.14 | Apache-2.0 AND ISC |
| roxmltree | 0.20.0 | MIT OR Apache-2.0 |
| rustc-hash | 1.1.0 | Apache-2.0/MIT |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT |
| rustls | 0.23.45 | Apache-2.0 OR ISC OR MIT |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 |
| rustls-webpki | 0.103.15 | ISC |
| rustybuzz | 0.20.1 | MIT |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 |
| self_cell | 1.3.0 | Apache-2.0 OR GPL-2.0-only |
| semver | 1.0.28 | MIT OR Apache-2.0 |
| serde | 1.0.229 | MIT OR Apache-2.0 |
| serde_core | 1.0.229 | MIT OR Apache-2.0 |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 |
| serde_json | 1.0.151 | MIT OR Apache-2.0 |
| serde_urlencoded | 0.7.1 | MIT/Apache-2.0 |
| sha2 | 0.10.9 | MIT OR Apache-2.0 |
| signature | 2.2.0 | Apache-2.0 OR MIT |
| simd-adler32 | 0.3.10 | MIT |
| simplecss | 0.2.2 | Apache-2.0 OR MIT |
| siphasher | 1.0.4 | MIT OR Apache-2.0 |
| skrifa | 0.37.0 | MIT OR Apache-2.0 |
| skrifa | 0.44.0 | MIT OR Apache-2.0 |
| slab | 0.4.12 | MIT |
| slotmap | 1.1.1 | Zlib |
| smallvec | 1.16.2 | MIT OR Apache-2.0 |
| smol_str | 0.2.2 | MIT OR Apache-2.0 |
| socket2 | 0.6.5 | MIT OR Apache-2.0 |
| softbuffer | 0.4.8 | MIT OR Apache-2.0 |
| spirv | 0.3.0+sdk-1.3.268.0 | Apache-2.0 |
| spki | 0.7.3 | Apache-2.0 OR MIT |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 |
| strict-num | 0.1.1 | MIT |
| strsim | 0.11.1 | MIT |
| subtle | 2.6.1 | BSD-3-Clause |
| svg_fmt | 0.4.5 | MIT/Apache-2.0 |
| svgtypes | 0.15.3 | Apache-2.0 OR MIT |
| swash | 0.2.10 | Apache-2.0 OR MIT |
| syn | 2.0.119 | MIT OR Apache-2.0 |
| syn | 3.0.6 | MIT OR Apache-2.0 |
| sync_wrapper | 1.0.2 | Apache-2.0 |
| synstructure | 0.14.0 | MIT |
| sys-locale | 0.3.2 | MIT OR Apache-2.0 |
| termcolor | 1.4.1 | Unlicense OR MIT |
| thiserror | 1.0.69 | MIT OR Apache-2.0 |
| thiserror | 2.0.21 | MIT OR Apache-2.0 |
| thiserror-impl | 1.0.69 | MIT OR Apache-2.0 |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 |
| tiny-skia | 0.11.4 | BSD-3-Clause |
| tiny-skia-path | 0.11.4 | BSD-3-Clause |
| tinystr | 0.8.4 | Unicode-3.0 |
| tinyvec | 1.13.3 | Zlib OR Apache-2.0 OR MIT |
| tokio | 1.53.1 | MIT |
| tokio-rustls | 0.26.6 | MIT OR Apache-2.0 |
| tokio-util | 0.7.19 | MIT |
| tower | 0.5.3 | MIT |
| tower-http | 0.6.11 | MIT |
| tower-layer | 0.3.3 | MIT |
| tower-service | 0.3.3 | MIT |
| tracing | 0.1.44 | MIT |
| tracing-attributes | 0.1.31 | MIT |
| tracing-core | 0.1.36 | MIT |
| try-lock | 0.2.5 | MIT |
| ttf-parser | 0.25.1 | MIT OR Apache-2.0 |
| typenum | 1.20.1 | MIT OR Apache-2.0 |
| unicode-bidi | 0.3.18 | MIT OR Apache-2.0 |
| unicode-bidi-mirroring | 0.4.0 | MIT/Apache-2.0 |
| unicode-ccc | 0.4.0 | MIT/Apache-2.0 |
| unicode-ident | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| unicode-linebreak | 0.1.5 | Apache-2.0 |
| unicode-properties | 0.1.4 | MIT/Apache-2.0 |
| unicode-script | 0.5.8 | MIT OR Apache-2.0 |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 |
| unicode-vo | 0.1.0 | MIT/Apache-2.0 |
| unicode-width | 0.2.2 | MIT OR Apache-2.0 |
| untrusted | 0.9.0 | ISC |
| url | 2.5.8 | MIT OR Apache-2.0 |
| usvg | 0.45.1 | Apache-2.0 OR MIT |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT |
| utf8parse | 0.2.2 | Apache-2.0 OR MIT |
| uuid | 1.26.1 | Apache-2.0 OR MIT |
| want | 0.3.1 | MIT |
| web-time | 1.1.0 | MIT OR Apache-2.0 |
| webpki-roots | 1.0.9 | CDLA-Permissive-2.0 |
| weezl | 0.1.12 | MIT OR Apache-2.0 |
| wgpu | 27.0.1 | MIT OR Apache-2.0 |
| wgpu-core | 27.0.3 | MIT OR Apache-2.0 |
| wgpu-core-deps-windows-linux-android | 27.0.0 | MIT OR Apache-2.0 |
| wgpu-hal | 27.0.4 | MIT OR Apache-2.0 |
| wgpu-types | 27.0.1 | MIT OR Apache-2.0 |
| widestring | 1.2.1 | MIT OR Apache-2.0 |
| winapi | 0.3.9 | MIT/Apache-2.0 |
| winapi-util | 0.1.11 | Unlicense OR MIT |
| window_clipboard | 0.5.1 | MIT |
| windows | 0.58.0 | MIT OR Apache-2.0 |
| windows-core | 0.58.0 | MIT OR Apache-2.0 |
| windows-implement | 0.58.0 | MIT OR Apache-2.0 |
| windows-interface | 0.58.0 | MIT OR Apache-2.0 |
| windows-link | 0.2.1 | MIT OR Apache-2.0 |
| windows-result | 0.2.0 | MIT OR Apache-2.0 |
| windows-service | 0.8.1 | MIT OR Apache-2.0 |
| windows-strings | 0.1.0 | MIT OR Apache-2.0 |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 |
| windows-sys | 0.59.0 | MIT OR Apache-2.0 |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 |
| winit | 0.30.13 | Apache-2.0 |
| writeable | 0.6.4 | Unicode-3.0 |
| xmlwriter | 0.1.0 | MIT |
| yazi | 0.2.1 | Apache-2.0 OR MIT |
| yoke | 0.8.3 | Unicode-3.0 |
| yoke-derive | 0.8.4 | Unicode-3.0 |
| zeno | 0.3.3 | Apache-2.0 OR MIT |
| zerocopy | 0.8.59 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zerocopy-derive | 0.8.59 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zerofrom | 0.1.8 | Unicode-3.0 |
| zerofrom-derive | 0.1.8 | Unicode-3.0 |
| zeroize | 1.9.0 | Apache-2.0 OR MIT |
| zerotrie | 0.2.5 | Unicode-3.0 |
| zerovec | 0.11.8 | Unicode-3.0 |
| zerovec-derive | 0.11.6 | Unicode-3.0 |
| zlib-rs | 0.6.8 | Zlib |
| zmij | 1.0.23 | MIT |
| zune-core | 0.4.12 | MIT OR Apache-2.0 OR Zlib |
| zune-jpeg | 0.4.21 | MIT OR Apache-2.0 OR Zlib |
