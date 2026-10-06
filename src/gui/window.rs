//! The window icon, decoded from the bundled .ico.

pub fn window_icon_rgba(size: u32) -> Vec<u8> {
    const ICO: &[u8] = include_bytes!("../../assets/secblitz.ico");
    ico_frame(ICO, size).unwrap_or_default()
}

fn ico_frame(ico: &[u8], size: u32) -> Option<Vec<u8>> {
    let count = u16::from_le_bytes([*ico.get(4)?, *ico.get(5)?]) as usize;
    for i in 0..count {
        let entry = ico.get(6 + 16 * i..22 + 16 * i)?;
        let width = if entry[0] == 0 {
            256
        } else {
            u32::from(entry[0])
        };
        if width != size {
            continue;
        }
        let len = u32::from_le_bytes(entry[8..12].try_into().ok()?) as usize;
        let offset = u32::from_le_bytes(entry[12..16].try_into().ok()?) as usize;
        let mut reader = png::Decoder::new(ico.get(offset..offset.checked_add(len)?)?)
            .read_info()
            .ok()?;
        let mut pixels = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut pixels).ok()?;
        let rgba = frame.color_type == png::ColorType::Rgba
            && frame.bit_depth == png::BitDepth::Eight
            && frame.width == size
            && frame.height == size;
        pixels.truncate(frame.buffer_size());
        return rgba.then_some(pixels);
    }
    None
}

pub fn window_icon() -> Option<iced::window::Icon> {
    iced::window::icon::from_rgba(window_icon_rgba(64), 64, 64).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_has_expected_size_and_shape() {
        let px = window_icon_rgba(32);
        assert_eq!(px.len(), 32 * 32 * 4);
        let at = |x: usize, y: usize| &px[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
        assert_eq!(at(0, 0)[3], 0, "corner is transparent");
        assert_eq!(at(7, 13)[..3], [255, 255, 255], "shield outline is white");
        assert_eq!(at(3, 16)[..3], [0x18, 0x18, 0x1B], "the tile is ink");
        assert!(
            at(11, 13)[..3].iter().all(|&c| c < 0x30),
            "inside the shield is dark"
        );
        assert_eq!(at(15, 11)[..3], [255, 255, 255], "the bolt is white");
        assert_eq!(window_icon_rgba(64).len(), 64 * 64 * 4);
        assert!(window_icon_rgba(33).is_empty(), "no frame, no icon");
        assert!(window_icon().is_some());
    }
}
