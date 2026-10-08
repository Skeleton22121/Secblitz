//! Each catalog app's own Windows icon, for the Clean up apps lists.
use super::backup::{self, parse_full_name, Manifest, Store};
use super::Installed;
use anyhow::{bail, ensure, Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

const MAX_PNG: u64 = 1024 * 1024;
const MAX_MANIFEST: u64 = 2 * 1024 * 1024;
const MAX_SIDE: u32 = 512;
const CACHE_DIR: &str = "icons";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Windows 10's tile blue, behind white app icons whose own tile color is
/// transparent or unknown.
const WINDOWS_BLUE: [u8; 3] = [0x00, 0x78, 0xD4];

fn visual_elements(manifest_xml: &str) -> Option<&str> {
    let mut rest = manifest_xml;
    loop {
        let at = rest.find("VisualElements")?;
        let before = &rest[..at];
        let after = &rest[at + "VisualElements".len()..];
        let opens = before
            .rfind('<')
            .map(|lt| {
                let name = &before[lt + 1..];
                !name.starts_with('/') && !name.contains(['>', ' ', '\n', '\r', '\t'])
            })
            .unwrap_or(false);
        if opens && after.starts_with(|c: char| c.is_ascii_whitespace()) {
            return Some(&after[..after.find('>')?]);
        }
        rest = after;
    }
}

pub fn logo_base(manifest_xml: &str) -> Option<String> {
    let tag = visual_elements(manifest_xml)?;
    let value = attribute(tag, "Square44x44Logo")?;
    let path = value.replace('\\', "/");
    let lower = path.to_ascii_lowercase();
    let base = &path[..lower.strip_suffix(".png")?.len()];
    (backup::valid_relative(&path) && backup::valid_relative(base)).then(|| base.to_string())
}

/// The app's own tile color, when the manifest names a dark enough one for a
/// white icon to show on.
pub fn tile_color(manifest_xml: &str) -> Option<[u8; 3]> {
    let value = attribute(visual_elements(manifest_xml)?, "BackgroundColor")?;
    let hex = value.trim().strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    let rgb = [channel(0)?, channel(2)?, channel(4)?];
    let luma = (299 * rgb[0] as u32 + 587 * rgb[1] as u32 + 114 * rgb[2] as u32) / 1000;
    (luma <= 160).then_some(rgb)
}

/// Windows 10 apps often ship white icons made for a colored tile or a dark
/// taskbar. They vanish on a light page.
pub fn light_glyph(image: &Rgba) -> bool {
    let (mut opaque, mut white) = (0usize, 0usize);
    for p in image.pixels.chunks_exact(4).filter(|p| p[3] >= 128) {
        opaque += 1;
        if p[0].min(p[1]).min(p[2]) >= 220 {
            white += 1;
        }
    }
    opaque > 0 && white * 10 >= opaque * 9
}

/// Draw the icon on a rounded tile of `color`, the way Windows 10 shows it.
pub fn plate(image: &Rgba, color: [u8; 3]) -> Rgba {
    let (w, h) = (image.width as f32, image.height as f32);
    let radius = w.min(h) * 0.2;
    let mut pixels = Vec::with_capacity(image.pixels.len());
    for (i, p) in image.pixels.chunks_exact(4).enumerate() {
        let x = (i as u32 % image.width) as f32 + 0.5;
        let y = (i as u32 / image.width) as f32 + 0.5;
        let dx = (radius - x).max(x - (w - radius)).max(0.0);
        let dy = (radius - y).max(y - (h - radius)).max(0.0);
        let cover = (radius - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
        let a = p[3] as f32 / 255.0;
        let alpha = a + cover * (1.0 - a);
        for c in 0..3 {
            let over = p[c] as f32 * a + color[c] as f32 * cover * (1.0 - a);
            pixels.push(if alpha > 0.0 {
                (over / alpha).round() as u8
            } else {
                0
            });
        }
        pixels.push((alpha * 255.0).round() as u8);
    }
    Rgba {
        width: image.width,
        height: image.height,
        pixels,
    }
}

fn encode_rgba(image: &Rgba) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().context("Save icon")?;
    writer
        .write_image_data(&image.pixels)
        .context("Save icon")?;
    writer.finish().context("Save icon")?;
    Ok(out)
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(i) = tag[from..].find(name) {
        let at = from + i;
        from = at + name.len();
        let boundary = tag[..at].ends_with(|c: char| c.is_ascii_whitespace());
        let tail = tag[from..].trim_start();
        if !boundary || !tail.starts_with('=') {
            continue;
        }
        let tail = tail[1..].trim_start();
        let quote = tail.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let inner = &tail[1..];
        return inner.find(quote).map(|end| &inner[..end]);
    }
    None
}

pub fn pick(names: &[String], stem: &str) -> Option<String> {
    let prefix = format!("{}.", stem.to_ascii_lowercase());
    let cands: Vec<(String, &String)> = names
        .iter()
        .filter_map(|n| {
            let lower = n.to_ascii_lowercase();
            let q = lower.strip_prefix(&prefix)?.strip_suffix(".png")?;
            (!lower.contains("contrast-")).then(|| (q.to_string(), n))
        })
        .collect();
    let find = |q: &str| {
        cands
            .iter()
            .find(|(c, _)| c == q)
            .map(|(_, n)| (*n).clone())
    };
    for alt in ["_altform-unplated", "_altform-lightunplated", ""] {
        if let Some(n) = find(&format!("targetsize-48{alt}")) {
            return Some(n);
        }
        let smallest = cands
            .iter()
            .filter_map(|(q, _)| {
                let size = q.strip_prefix("targetsize-")?.strip_suffix(alt)?;
                size.parse::<u32>().ok().filter(|s| *s >= 32 && *s != 256)
            })
            .min();
        if let Some(n) = smallest.and_then(|s| find(&format!("targetsize-{s}{alt}"))) {
            return Some(n);
        }
        if let Some(n) = find(&format!("targetsize-256{alt}")) {
            return Some(n);
        }
    }
    for q in ["scale-200", "scale-100"] {
        if let Some(n) = find(q) {
            return Some(n);
        }
    }
    names
        .iter()
        .find(|n| n.to_ascii_lowercase() == format!("{prefix}png"))
        .cloned()
}

pub fn decode(bytes: &[u8]) -> Result<Rgba> {
    ensure!(bytes.len() as u64 <= MAX_PNG, "Icon file is too big");
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().context("Read icon")?;
    let (w, h) = {
        let info = reader.info();
        (info.width, info.height)
    };
    ensure!(
        (1..=MAX_SIDE).contains(&w) && (1..=MAX_SIDE).contains(&h),
        "Icon size is not supported"
    );
    let mut buf = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf).context("Read icon")?;
    ensure!(
        frame.bit_depth == png::BitDepth::Eight,
        "Icon format is not supported"
    );
    let data = &buf[..frame.buffer_size()];
    let pixels: Vec<u8> = match frame.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::Indexed => bail!("Icon format is not supported"),
    };
    ensure!(
        pixels.len() == w as usize * h as usize * 4,
        "Icon data is incomplete"
    );
    Ok(Rgba {
        width: w,
        height: h,
        pixels,
    })
}

/// Read a regular file of at most `cap` bytes. `strict` also refuses
/// junctions and other reparse points (our own folders); package folders only
/// refuse symlinks, because Windows may compress some of their files.
fn read_capped(path: &Path, cap: u64, strict: bool) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink() && (!strict || backup::plain(&meta)),
        "Unexpected file"
    );
    ensure!(meta.len() <= cap, "File is too big");
    let mut out = Vec::new();
    fs::File::open(path)?.take(cap + 1).read_to_end(&mut out)?;
    ensure!(out.len() as u64 <= cap, "File is too big");
    Ok(out)
}

fn is_dir(path: &Path, strict: bool) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.is_dir() && !m.file_type().is_symlink() && (!strict || backup::plain(&m)))
        .unwrap_or(false)
}

fn file_names(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default()
}

pub fn from_package(main: &Path, resources: &[PathBuf], strict: bool) -> Result<(Vec<u8>, Rgba)> {
    let manifest = read_capped(&main.join("AppxManifest.xml"), MAX_MANIFEST, strict)?;
    let manifest = String::from_utf8_lossy(&manifest);
    let base = logo_base(&manifest).context("No app icon in the manifest")?;
    let (dir, stem) = match base.rsplit_once('/') {
        Some((d, s)) => (d, s),
        None => ("", base.as_str()),
    };
    let folders: Vec<&Path> = std::iter::once(main)
        .chain(resources.iter().map(PathBuf::as_path))
        .collect();
    let assets_of = |folder: &Path| {
        if dir.is_empty() {
            folder.to_path_buf()
        } else {
            folder.join(dir)
        }
    };
    // Some Windows 10 apps ship only high-contrast icons. The contrast-black
    // set is a white glyph, which gets plated like any other white icon.
    let found = ["", "contrast-black"].iter().find_map(|sub| {
        let mut names: Vec<(String, PathBuf)> = Vec::new();
        for folder in &folders {
            let assets = assets_of(folder).join(sub);
            if is_dir(&assets, strict) {
                names.extend(file_names(&assets).into_iter().map(|n| (n, assets.clone())));
            }
        }
        let all: Vec<String> = names.iter().map(|(n, _)| n.clone()).collect();
        let chosen = pick(&all, stem)?;
        names.into_iter().find(|(n, _)| *n == chosen)
    });
    let (chosen, dir) = found.context("No icon file")?;
    let bytes = read_capped(&dir.join(&chosen), MAX_PNG, strict)?;
    let image = decode(&bytes)?;
    if light_glyph(&image) {
        let plated = plate(&image, tile_color(&manifest).unwrap_or(WINDOWS_BLUE));
        return Ok((encode_rgba(&plated)?, plated));
    }
    Ok((bytes, image))
}

fn version_key(version: &str) -> Vec<u16> {
    version.split('.').filter_map(|n| n.parse().ok()).collect()
}

pub fn installed_folders(root: &Path, package: &str) -> Option<(PathBuf, Vec<PathBuf>)> {
    let mut found: Vec<(Vec<u16>, backup::Identity, PathBuf)> = Vec::new();
    for entry in fs::read_dir(root).ok()?.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let Ok(id) = parse_full_name(&name) else {
            continue;
        };
        if id.name.eq_ignore_ascii_case(package) && is_dir(&entry.path(), false) {
            found.push((version_key(&id.version), id, entry.path()));
        }
    }
    let newest = found
        .iter()
        .filter(|(_, id, _)| id.resource.is_empty())
        .max_by(|a, b| a.0.cmp(&b.0))?;
    let resources = found
        .iter()
        .filter(|(_, id, _)| !id.resource.is_empty() && id.version == newest.1.version)
        .map(|(_, _, p)| p.clone())
        .collect();
    Some((newest.2.clone(), resources))
}

pub fn saved_folders(store: &Store, m: &Manifest) -> Option<(PathBuf, Vec<PathBuf>)> {
    let ids: Vec<(backup::Identity, PathBuf)> = m
        .packages
        .iter()
        .filter(|p| p.kind != backup::Kind::Framework)
        .filter_map(|p| {
            let id = parse_full_name(&p.full_name).ok()?;
            let dir = store
                .family_dir(&m.family)
                .join(backup::PACKAGES)
                .join(&p.full_name);
            Some((id, dir))
        })
        .collect();
    let main = ids.iter().find(|(id, _)| id.resource.is_empty())?;
    let resources = ids
        .iter()
        .filter(|(id, _)| {
            !id.resource.is_empty() && id.name == main.0.name && id.version == main.0.version
        })
        .map(|(_, d)| d.clone())
        .collect();
    Some((main.1.clone(), resources))
}

fn cache_file(app_dir: &Path, index: u16) -> PathBuf {
    app_dir.join(CACHE_DIR).join(format!("{index}.png"))
}

fn cache_read(app_dir: &Path, index: u16) -> Option<Rgba> {
    let bytes = read_capped(&cache_file(app_dir, index), MAX_PNG, true).ok()?;
    decode(&bytes).ok()
}

fn cache_write(app_dir: &Path, index: u16, bytes: &[u8]) -> Result<()> {
    let dir = app_dir.join(CACHE_DIR);
    match fs::symlink_metadata(&dir) {
        Ok(m) => ensure!(m.is_dir() && backup::plain(&m), "Icon cache is not plain"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&dir)?,
        Err(e) => return Err(e.into()),
    }
    let target = cache_file(app_dir, index);
    if let Ok(meta) = fs::symlink_metadata(&target) {
        ensure!(
            meta.is_file() && backup::plain(&meta),
            "Icon cache is not plain"
        );
        if read_capped(&target, MAX_PNG, true).ok().as_deref() == Some(bytes) {
            return Ok(());
        }
    }
    let temp = dir.join(format!("{index}.png.tmp"));
    if let Ok(meta) = fs::symlink_metadata(&temp) {
        ensure!(
            meta.is_file() && backup::plain(&meta),
            "Icon cache is not plain"
        );
        fs::remove_file(&temp)?;
    }
    fs::write(&temp, bytes)?;
    fs::rename(&temp, &target).inspect_err(|_| {
        let _ = fs::remove_file(&temp);
    })?;
    Ok(())
}

pub struct Sources<'a> {
    pub root: Option<&'a Path>,
    pub store: Option<&'a Store>,
    pub app_dir: Option<&'a Path>,
}

pub fn load_from(src: &Sources, installed: &[Installed], catalog_len: u16) -> BTreeMap<u16, Rgba> {
    let mut out = BTreeMap::new();
    for index in 0..catalog_len {
        if let Some(icon) = one(src, installed, index) {
            out.insert(index, icon);
        }
    }
    out
}

fn one(src: &Sources, installed: &[Installed], index: u16) -> Option<Rgba> {
    let mut packages: Vec<&Installed> = installed.iter().filter(|p| p.index == index).collect();
    packages.sort_by(|a, b| a.package.cmp(&b.package));
    if let Some(root) = src.root {
        for p in packages {
            let Some((main, res)) = installed_folders(root, &p.package) else {
                continue;
            };
            if let Ok((bytes, image)) = from_package(&main, &res, false) {
                if let Some(dir) = src.app_dir {
                    let _ = cache_write(dir, index, &bytes);
                }
                return Some(image);
            }
        }
    }
    if let Some(icon) = src.app_dir.and_then(|d| cache_read(d, index)) {
        if light_glyph(&icon) {
            return Some(plate(&icon, WINDOWS_BLUE));
        }
        return Some(icon);
    }
    let store = src.store?;
    for m in store.for_index(index) {
        let Some((main, res)) = saved_folders(store, &m) else {
            continue;
        };
        if let Ok((bytes, image)) = from_package(&main, &res, true) {
            if let Some(dir) = src.app_dir {
                let _ = cache_write(dir, index, &bytes);
            }
            return Some(image);
        }
    }
    None
}

pub fn load(installed: &[Installed]) -> BTreeMap<u16, Rgba> {
    let app_dir = crate::platform::app_dir().ok();
    let store = Store::open().ok();
    #[cfg(windows)]
    let root = super::winfs::windows_apps().ok();
    #[cfg(not(windows))]
    let root: Option<PathBuf> = None;
    let src = Sources {
        root: root.as_deref(),
        store: store.as_ref(),
        app_dir: app_dir.as_deref(),
    };
    let len = super::catalog().len().min(u16::MAX as usize) as u16;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        load_from(&src, installed, len)
    }))
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn logo_base_reads_first_visual_elements() {
        let xml = r#"<Package><Applications><Application Id="App">
            <uap:VisualElements DisplayName="x" Square150x150Logo="Assets\Big.png" Square44x44Logo="Assets\WeatherAppList.png" />
            </Application></Applications></Package>"#;
        assert_eq!(logo_base(xml).as_deref(), Some("Assets/WeatherAppList"));
        let single = "<VisualElements Square44x44Logo='Logo.PNG'/>";
        assert_eq!(logo_base(single).as_deref(), Some("Logo"));
    }

    #[test]
    fn logo_base_rejects_bad_values() {
        for bad in [
            "<uap:VisualElements Square44x44Logo=\"..\\x.png\"/>",
            "<uap:VisualElements Square44x44Logo=\"Assets\\x.jpg\"/>",
            "<uap:VisualElements Square44x44Logo=\"C:\\x.png\"/>",
            "<uap:VisualElements Square150x150Logo=\"a.png\"/>",
            "<uap:VisualElements Square44x44Logo=\"\"/>",
            "<Package/>",
            "",
        ] {
            assert_eq!(logo_base(bad), None, "{bad}");
        }
        let two = "<uap:VisualElements A=\"1\"/><uap:VisualElements Square44x44Logo=\"a.png\"/>";
        assert_eq!(logo_base(two), None);
    }

    #[test]
    fn pick_follows_the_preference_order() {
        let stem = "MediaPlayerAppList";
        let set = names(&[
            "MediaPlayerAppList.scale-200.png",
            "MediaPlayerAppList.targetsize-16.png",
            "MediaPlayerAppList.targetsize-24_altform-unplated.png",
            "MediaPlayerAppList.targetsize-256.png",
            "MediaPlayerAppList.targetsize-256_altform-lightunplated.png",
        ]);
        assert_eq!(
            pick(&set, stem).as_deref(),
            Some("MediaPlayerAppList.targetsize-256_altform-lightunplated.png")
        );
        let cases: &[(&[&str], &str)] = &[
            (
                &[
                    "A.targetsize-48.png",
                    "A.targetsize-48_altform-unplated.png",
                    "A.targetsize-256_altform-unplated.png",
                ],
                "A.targetsize-48_altform-unplated.png",
            ),
            (
                &[
                    "A.targetsize-256_altform-unplated.png",
                    "A.targetsize-32_altform-unplated.png",
                    "A.targetsize-24_altform-unplated.png",
                ],
                "A.targetsize-32_altform-unplated.png",
            ),
            (
                &[
                    "A.targetsize-256_altform-unplated.png",
                    "A.targetsize-48.png",
                ],
                "A.targetsize-256_altform-unplated.png",
            ),
            (
                &[
                    "A.targetsize-16.png",
                    "A.targetsize-32.png",
                    "A.scale-200.png",
                ],
                "A.targetsize-32.png",
            ),
            (
                &["A.targetsize-16.png", "A.scale-100.png", "A.scale-200.png"],
                "A.scale-200.png",
            ),
            (&["A.scale-100.png", "A.png"], "A.scale-100.png"),
            (&["A.png", "A.targetsize-16.png"], "A.png"),
        ];
        for (files, want) in cases {
            assert_eq!(
                pick(&names(files), "A").as_deref(),
                Some(*want),
                "{files:?}"
            );
        }
    }

    #[test]
    fn pick_ignores_contrast_and_other_stems() {
        let set = names(&[
            "CalculatorAppList.targetsize-16_altform-unplated_contrast-black.png",
            "CalculatorAppList.targetsize-48_contrast-white.png",
            "CalculatorAppListBig.targetsize-48.png",
            "Other.targetsize-48.png",
            "CalculatorAppList.targetsize-48.txt",
        ]);
        assert_eq!(pick(&set, "CalculatorAppList"), None);
        let mut set = set;
        set.push("CalculatorAppList.scale-100.png".into());
        assert_eq!(
            pick(&set, "CalculatorAppList").as_deref(),
            Some("CalculatorAppList.scale-100.png")
        );
    }

    fn encode(w: u32, h: u32, color: png::ColorType, depth: png::BitDepth, data: &[u8]) -> Vec<u8> {
        encode_with(w, h, color, depth, data, None)
    }

    fn encode_with(
        w: u32,
        h: u32,
        color: png::ColorType,
        depth: png::BitDepth,
        data: &[u8],
        palette: Option<Vec<u8>>,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(color);
        enc.set_depth(depth);
        if let Some(p) = palette {
            enc.set_palette(p);
        }
        let mut writer = enc.write_header().unwrap();
        writer.write_image_data(data).unwrap();
        writer.finish().unwrap();
        out
    }

    #[test]
    fn decode_converts_every_color_type() {
        use png::{BitDepth::*, ColorType::*};
        let rgba = encode(2, 1, Rgba, Eight, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(decode(&rgba).unwrap().pixels, [1, 2, 3, 4, 5, 6, 7, 8]);
        let rgb = encode(2, 1, Rgb, Eight, &[1, 2, 3, 4, 5, 6]);
        assert_eq!(decode(&rgb).unwrap().pixels, [1, 2, 3, 255, 4, 5, 6, 255]);
        let gray = encode(2, 1, Grayscale, Eight, &[9, 10]);
        assert_eq!(
            decode(&gray).unwrap().pixels,
            [9, 9, 9, 255, 10, 10, 10, 255]
        );
        let ga = encode(1, 1, GrayscaleAlpha, Eight, &[9, 100]);
        assert_eq!(decode(&ga).unwrap().pixels, [9, 9, 9, 100]);
        let sixteen = encode(1, 1, Rgb, Sixteen, &[10, 0, 20, 0, 30, 0]);
        assert_eq!(decode(&sixteen).unwrap().pixels, [10, 20, 30, 255]);
        let pal = encode_with(
            2,
            1,
            Indexed,
            Eight,
            &[1, 0],
            Some(vec![0, 0, 0, 200, 100, 50]),
        );
        let img = decode(&pal).unwrap();
        assert_eq!((img.width, img.height), (2, 1));
        assert_eq!(img.pixels, [200, 100, 50, 255, 0, 0, 0, 255]);
        let two_bit = encode(4, 1, Grayscale, Two, &[0b00_01_10_11]);
        assert_eq!(decode(&two_bit).unwrap().pixels.len(), 16);
    }

    #[test]
    fn decode_rejects_junk_and_oversize() {
        assert!(decode(b"not a png").is_err());
        assert!(decode(&[]).is_err());
        let wide = encode(
            513,
            1,
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
            &vec![0; 513],
        );
        assert!(decode(&wide).is_err());
        let ok = encode(
            512,
            1,
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
            &vec![0; 512],
        );
        assert!(decode(&ok).is_ok());
        let big = vec![0u8; MAX_PNG as usize + 1];
        assert!(decode(&big).is_err());
        let mut truncated = encode(4, 4, png::ColorType::Rgba, png::BitDepth::Eight, &[7; 64]);
        truncated.truncate(truncated.len() - 20);
        assert!(decode(&truncated).is_err());
    }

    fn tiny() -> Vec<u8> {
        encode(
            1,
            1,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &[9, 8, 7, 255],
        )
    }

    fn package(root: &Path, full: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let dir = root.join(full);
        for (rel, bytes) in files {
            let p = dir.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, bytes).unwrap();
        }
        dir
    }

    const MANIFEST: &str =
        "<Package><uap:VisualElements Square44x44Logo=\"Assets\\W.png\"/></Package>";

    #[test]
    fn installed_icon_uses_newest_version_and_resource_folders() {
        let dir = tempfile::tempdir().unwrap();
        let png = tiny();
        let main_old = "Microsoft.BingWeather_4.1.0.0_x64__8wekyb3d8bbwe";
        let main = "Microsoft.BingWeather_4.54.0.0_x64__8wekyb3d8bbwe";
        let res = "Microsoft.BingWeather_4.54.0.0_neutral_split.scale-100_8wekyb3d8bbwe";
        let res_old = "Microsoft.BingWeather_4.1.0.0_neutral_split.scale-100_8wekyb3d8bbwe";
        package(dir.path(), main_old, &[("AppxManifest.xml", b"<Package/>")]);
        package(dir.path(), res_old, &[("Assets/W.scale-100.png", b"junk")]);
        package(
            dir.path(),
            main,
            &[
                ("AppxManifest.xml", MANIFEST.as_bytes()),
                ("Assets/W.targetsize-16.png", &png),
            ],
        );
        package(dir.path(), res, &[("Assets/W.scale-100.png", &png)]);
        package(dir.path(), "Other.App_1.0.0.0_x64__8wekyb3d8bbwe", &[]);
        let (m, r) = installed_folders(dir.path(), "Microsoft.BingWeather").unwrap();
        assert_eq!(m, dir.path().join(main));
        assert_eq!(r, vec![dir.path().join(res)]);
        let (bytes, img) = from_package(&m, &r, false).unwrap();
        assert_eq!(bytes, png);
        assert_eq!(img.pixels, [9, 8, 7, 255]);
        assert!(installed_folders(dir.path(), "Missing.App").is_none());
    }

    fn white_glyph(side: u32) -> Vec<u8> {
        let mut px = Vec::new();
        for y in 0..side {
            for x in 0..side {
                let inside =
                    (side / 4..side * 3 / 4).contains(&x) && (side / 4..side * 3 / 4).contains(&y);
                px.extend_from_slice(if inside {
                    &[255, 255, 255, 255]
                } else {
                    &[0, 0, 0, 0]
                });
            }
        }
        encode(side, side, png::ColorType::Rgba, png::BitDepth::Eight, &px)
    }

    fn at(img: &Rgba, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * img.width + x) * 4) as usize;
        img.pixels[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn tile_color_takes_only_dark_hex_colors() {
        let ve = |bg: &str| {
            format!("<uap:VisualElements BackgroundColor=\"{bg}\" Square44x44Logo=\"L.png\"/>")
        };
        assert_eq!(tile_color(&ve("#113768")), Some([0x11, 0x37, 0x68]));
        assert_eq!(tile_color(&ve("transparent")), None);
        assert_eq!(tile_color(&ve("#FFFFFF")), None);
        assert_eq!(tile_color(&ve("#12345")), None);
        assert_eq!(tile_color(&ve("#GG0000")), None);
        assert_eq!(tile_color("<Package/>"), None);
    }

    #[test]
    fn only_white_icons_count_as_light_glyphs() {
        assert!(light_glyph(&decode(&white_glyph(8)).unwrap()));
        assert!(!light_glyph(&decode(&tiny()).unwrap()));
        let clear = encode(
            1,
            1,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &[255, 255, 255, 0],
        );
        assert!(!light_glyph(&decode(&clear).unwrap()));
    }

    #[test]
    fn plate_puts_the_icon_on_a_rounded_tile() {
        let img = plate(&decode(&white_glyph(48)).unwrap(), [0, 0x78, 0xD4]);
        assert_eq!((img.width, img.height), (48, 48));
        assert_eq!(at(&img, 24, 24), [255, 255, 255, 255]);
        assert_eq!(at(&img, 24, 2), [0, 0x78, 0xD4, 255]);
        assert_eq!(at(&img, 0, 0)[3], 0);
        assert!(!light_glyph(&img));
        let round_trip = decode(&encode_rgba(&img).unwrap()).unwrap();
        assert_eq!(round_trip, img);
    }

    #[test]
    fn white_icons_are_plated_in_the_app_tile_color() {
        let dir = tempfile::tempdir().unwrap();
        let glyph = white_glyph(48);
        let manifest = "<Package><uap:VisualElements BackgroundColor=\"#113768\" Square44x44Logo=\"Assets\\W.png\"/></Package>";
        let main = package(
            dir.path(),
            "A.B_1.0.0.0_x64__8wekyb3d8bbwe",
            &[
                ("AppxManifest.xml", manifest.as_bytes()),
                ("Assets/W.targetsize-48_altform-unplated.png", &glyph),
            ],
        );
        let (bytes, img) = from_package(&main, &[], false).unwrap();
        assert_ne!(bytes, glyph);
        assert_eq!(decode(&bytes).unwrap(), img);
        assert_eq!(at(&img, 24, 2), [0x11, 0x37, 0x68, 255]);
        let plain = package(
            dir.path(),
            "C.D_1.0.0.0_x64__8wekyb3d8bbwe",
            &[
                ("AppxManifest.xml", MANIFEST.as_bytes()),
                ("Assets/W.targetsize-48.png", &glyph),
            ],
        );
        let (_, img) = from_package(&plain, &[], false).unwrap();
        assert_eq!(at(&img, 24, 2), [0, 0x78, 0xD4, 255]);
    }

    #[test]
    fn apps_with_only_high_contrast_icons_use_the_white_set_on_a_tile() {
        let dir = tempfile::tempdir().unwrap();
        let mut black = Vec::new();
        for _ in 0..48 * 48 {
            black.extend_from_slice(&[0, 0, 0, 255]);
        }
        let black = encode(48, 48, png::ColorType::Rgba, png::BitDepth::Eight, &black);
        let main = package(
            dir.path(),
            "A.B_1.0.0.0_x64__8wekyb3d8bbwe",
            &[
                ("AppxManifest.xml", MANIFEST.as_bytes()),
                ("Assets/contrast-white/W.targetsize-48.png", &black),
                (
                    "Assets/contrast-black/W.targetsize-48.png",
                    &white_glyph(48),
                ),
            ],
        );
        let (_, img) = from_package(&main, &[], false).unwrap();
        assert_eq!(at(&img, 24, 24), [255, 255, 255, 255]);
        assert_eq!(at(&img, 24, 2), [0, 0x78, 0xD4, 255]);
        let both = package(
            dir.path(),
            "C.D_1.0.0.0_x64__8wekyb3d8bbwe",
            &[
                ("AppxManifest.xml", MANIFEST.as_bytes()),
                ("Assets/W.targetsize-48.png", &tiny()),
                (
                    "Assets/contrast-black/W.targetsize-48.png",
                    &white_glyph(48),
                ),
            ],
        );
        assert_eq!(
            from_package(&both, &[], false).unwrap().1.pixels,
            [9, 8, 7, 255]
        );
    }

    #[test]
    fn junk_icon_file_is_not_used() {
        let dir = tempfile::tempdir().unwrap();
        let main = package(
            dir.path(),
            "A.B_1.0.0.0_x64__8wekyb3d8bbwe",
            &[
                ("AppxManifest.xml", MANIFEST.as_bytes()),
                ("Assets/W.targetsize-48_altform-unplated.png", b"junk"),
            ],
        );
        assert!(from_package(&main, &[], false).is_err());
    }

    #[test]
    fn cache_replaces_atomically_and_only_decodable_bytes_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let a = tiny();
        cache_write(dir.path(), 7, &a).unwrap();
        assert_eq!(cache_read(dir.path(), 7).unwrap().pixels, [9, 8, 7, 255]);
        let b = encode(
            1,
            1,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &[1, 1, 1, 255],
        );
        cache_write(dir.path(), 7, &b).unwrap();
        assert_eq!(cache_read(dir.path(), 7).unwrap().pixels, [1, 1, 1, 255]);
        let left: Vec<String> = file_names(&dir.path().join("icons"));
        assert_eq!(left, ["7.png"]);
        fs::write(dir.path().join("icons/8.png"), b"junk").unwrap();
        assert!(cache_read(dir.path(), 8).is_none());
        assert!(cache_read(dir.path(), 9).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn cache_refuses_links() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), dir.path().join("icons")).unwrap();
        assert!(cache_write(dir.path(), 1, &tiny()).is_err());
        assert!(fs::read_dir(elsewhere.path()).unwrap().next().is_none());
    }

    #[test]
    fn load_prefers_installed_then_falls_back_to_the_cache() {
        let apps = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let index = super::super::catalog::owner("Microsoft.BingWeather").unwrap();
        let png = tiny();
        package(
            apps.path(),
            "Microsoft.BingWeather_4.54.0.0_x64__8wekyb3d8bbwe",
            &[
                ("AppxManifest.xml", MANIFEST.as_bytes()),
                ("Assets/W.targetsize-48_altform-unplated.png", &png),
            ],
        );
        let installed = vec![Installed {
            index,
            package: "Microsoft.BingWeather".into(),
            version: "4.54.0.0".into(),
        }];
        let src = Sources {
            root: Some(apps.path()),
            store: None,
            app_dir: Some(state.path()),
        };
        let len = super::super::catalog().len() as u16;
        let got = load_from(&src, &installed, len);
        assert_eq!(got.len(), 1);
        assert_eq!(got[&index].pixels, [9, 8, 7, 255]);
        let got = load_from(&src, &[], len);
        assert_eq!(got[&index].pixels, [9, 8, 7, 255]);
        let none = Sources {
            root: None,
            store: None,
            app_dir: None,
        };
        assert!(load_from(&none, &installed, len).is_empty());
    }

    #[test]
    fn a_white_icon_cached_by_an_older_version_is_plated() {
        let state = tempfile::tempdir().unwrap();
        let index = super::super::catalog::owner("Microsoft.BingWeather").unwrap();
        cache_write(state.path(), index, &white_glyph(48)).unwrap();
        let src = Sources {
            root: None,
            store: None,
            app_dir: Some(state.path()),
        };
        let got = load_from(&src, &[], super::super::catalog().len() as u16);
        assert_eq!(at(&got[&index], 24, 2), [0, 0x78, 0xD4, 255]);
    }
}
