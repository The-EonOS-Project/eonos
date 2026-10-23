use alloc::vec::Vec;

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const MAX_PIXELS: usize = 4_194_304;
const MAX_COMPRESSED_BYTES: usize = 16 * 1024 * 1024;
const MAX_DECOMPRESSED_BYTES: usize = 32 * 1024 * 1024;

pub struct Image {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
}

pub fn decode(bytes: &[u8]) -> Result<Image, &'static str> {
    if bytes.len() < 8 || &bytes[..8] != PNG_SIGNATURE { return Err("invalid PNG signature"); }

    let mut offset = 8usize;
    let mut dimensions = None;
    let mut compressed = Vec::new();
    let mut saw_end = false;
    while offset.checked_add(12).is_some_and(|end| end <= bytes.len()) {
        let length = read_be_u32(bytes, offset)? as usize;
        let chunk_end = offset.checked_add(12).and_then(|v| v.checked_add(length)).ok_or("PNG chunk is too large")?;
        if chunk_end > bytes.len() { return Err("truncated PNG chunk"); }
        let kind = &bytes[offset + 4..offset + 8];
        let data = &bytes[offset + 8..offset + 8 + length];
        let expected_crc = read_be_u32(bytes, offset + 8 + length)?;
        if crc32(&bytes[offset + 4..offset + 8 + length]) != expected_crc {
            return Err("PNG chunk checksum failed");
        }
        match kind {
            b"IHDR" => {
                if dimensions.is_some() || length != 13 { return Err("invalid PNG header"); }
                let width = read_be_u32(data, 0)? as usize;
                let height = read_be_u32(data, 4)? as usize;
                let bit_depth = data[8];
                let color_type = data[9];
                if width == 0 || height == 0 || width.checked_mul(height).filter(|n| *n <= MAX_PIXELS).is_none() {
                    return Err("PNG dimensions exceed the wallpaper limit");
                }
                if bit_depth != 8 || !matches!(color_type, 2 | 6) || data[10] != 0 || data[11] != 0 || data[12] != 0 {
                    return Err("wallpapers must use non-interlaced 8-bit RGB or RGBA PNG");
                }
                dimensions = Some((width, height, color_type as usize));
            }
            b"IDAT" => {
                if dimensions.is_none() { return Err("PNG image data precedes its header"); }
                if compressed.len().checked_add(data.len()).filter(|n| *n <= MAX_COMPRESSED_BYTES).is_none() {
                    return Err("compressed PNG exceeds the wallpaper limit");
                }
                compressed.extend_from_slice(data);
            }
            b"IEND" => { saw_end = true; break; }
            b"PLTE" => {}
            _ if kind[0] & 0x20 == 0 => return Err("PNG contains an unsupported critical chunk"),
            _ => {}
        }
        offset = chunk_end;
    }
    if !saw_end { return Err("PNG is missing its end marker"); }
    let (width, height, color_type) = dimensions.ok_or("PNG is missing its header")?;
    let channels = if color_type == 6 { 4usize } else { 3usize };
    let stride = width.checked_mul(channels).ok_or("PNG row is too large")?;
    let decoded_len = stride.checked_add(1).and_then(|n| n.checked_mul(height)).ok_or("PNG image is too large")?;
    if decoded_len > MAX_DECOMPRESSED_BYTES { return Err("decoded PNG exceeds the wallpaper limit"); }
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&compressed, decoded_len)
        .map_err(|_| "PNG image data could not be decompressed")?;
    if raw.len() != decoded_len { return Err("PNG image data has an invalid size"); }

    let mut pixels = Vec::new();
    pixels.try_reserve_exact(width * height).map_err(|_| "not enough memory for PNG wallpaper")?;
    let mut previous = Vec::new();
    previous.try_reserve_exact(stride).map_err(|_| "not enough memory for PNG row")?;
    previous.resize(stride, 0u8);
    let mut row = Vec::new();
    row.try_reserve_exact(stride).map_err(|_| "not enough memory for PNG row")?;
    row.resize(stride, 0u8);

    for y in 0..height {
        let start = y * (stride + 1);
        let filter = raw[start];
        row.copy_from_slice(&raw[start + 1..start + 1 + stride]);
        for x in 0..stride {
            let left = if x >= channels { row[x - channels] } else { 0 };
            let above = previous[x];
            let upper_left = if x >= channels { previous[x - channels] } else { 0 };
            row[x] = row[x].wrapping_add(match filter {
                0 => 0,
                1 => left,
                2 => above,
                3 => ((left as u16 + above as u16) / 2) as u8,
                4 => paeth(left, above, upper_left),
                _ => return Err("PNG uses an unknown row filter"),
            });
        }
        for sample in row.chunks_exact(channels) {
            let (r, g, b) = (sample[0] as u32, sample[1] as u32, sample[2] as u32);
            let alpha = if channels == 4 { sample[3] as u32 } else { 255 };
            // Blend transparency over the existing Era midnight wallpaper color.
            let base = 0x111827u32;
            let br = (base >> 16) & 0xff;
            let bg = (base >> 8) & 0xff;
            let bb = base & 0xff;
            let r = (r * alpha + br * (255 - alpha)) / 255;
            let g = (g * alpha + bg * (255 - alpha)) / 255;
            let b = (b * alpha + bb * (255 - alpha)) / 255;
            pixels.push((r << 16) | (g << 8) | b);
        }
        core::mem::swap(&mut row, &mut previous);
    }
    Ok(Image { width, height, pixels })
}

fn read_be_u32(bytes: &[u8], offset: usize) -> Result<u32, &'static str> {
    let value = bytes.get(offset..offset + 4).ok_or("truncated PNG value")?;
    Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
}

fn paeth(left: u8, above: u8, upper_left: u8) -> u8 {
    let (a, b, c) = (left as i32, above as i32, upper_left as i32);
    let p = a + b - c;
    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
    if pa <= pb && pa <= pc { left } else if pb <= pc { above } else { upper_left }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}
