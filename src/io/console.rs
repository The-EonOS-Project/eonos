use crate::sync::IrqMutex;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use crate::framebuffer::FbInfo;

static FONT_DATA: &[u8] = include_bytes!("../../assets/fonts/font.psf");

const PALETTE: [u32; 16] = [
    0x001B1F27, 0x00E06C75, 0x0098C379, 0x00E5C07B, 0x0061AFEF, 0x00C678DD, 0x0056B6C2, 0x00ABB2BF,
    0x004B5263, 0x00FF7B86, 0x00B5E890, 0x00FFD98E, 0x0082C4FF, 0x00E09BFF, 0x007FDCE8, 0x00FFFFFF,
];

const DEFAULT_FG: u32 = PALETTE[7];
const DEFAULT_BG: u32 = 0x00000000;

pub static CONSOLE: IrqMutex<Option<Console>> = IrqMutex::new(None);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphicsError {
    Busy,
    OutOfMemory,
}

#[derive(Clone, Copy)]
struct Font {
    glyphs: &'static [u8],
    unicode_table: &'static [u8],
    num_glyphs: usize,
    charsize: usize,
    width: usize,
    height: usize,
    bytes_per_row: usize,
    ascii_glyphs: [u16; 128],
}

impl Font {
    fn parse(data: &'static [u8]) -> Option<Font> {
        let (headersize, num_glyphs, charsize, width, height, has_unicode_table) =
            if data.len() >= 32 && data.starts_with(&[0x72, 0xb5, 0x4a, 0x86]) {
                let rd = |o: usize| {
                    u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]) as usize
                };
                (rd(8), rd(16), rd(20), rd(28), rd(24), rd(12) & 1 != 0)
            } else if data.len() >= 4 && data[0] == 0x36 && data[1] == 0x04 {
                let num = if data[2] & 1 != 0 { 512 } else { 256 };
                (4, num, data[3] as usize, 8, data[3] as usize, false)
            } else {
                return None;
            };

        let bytes_per_row = (width + 7) / 8;
        if width == 0 || height == 0 || num_glyphs == 0 || charsize < bytes_per_row * height {
            return None;
        }
        let end = headersize.checked_add(num_glyphs.checked_mul(charsize)?)?;
        if end > data.len() {
            return None;
        }
        let unicode_table = if has_unicode_table {
            &data[end..]
        } else {
            &data[end..end]
        };

        
        
        
        let mut ascii_glyphs = [u16::MAX; 128];
        if has_unicode_table {
            let mut pos = 0;
            for glyph_idx in 0..num_glyphs {
                let Some(relative_end) = unicode_table[pos..].iter().position(|&b| b == 0xff)
                else {
                    break;
                };
                let glyph_end = pos + relative_end;
                let mut token_start = pos;
                for token_end in pos..=glyph_end {
                    if token_end == glyph_end || unicode_table[token_end] == 0xfe {
                        if token_end == token_start + 1 {
                            let codepoint = unicode_table[token_start] as usize;
                            if codepoint < ascii_glyphs.len()
                                && glyph_idx <= u16::MAX as usize
                                && ascii_glyphs[codepoint] == u16::MAX
                            {
                                ascii_glyphs[codepoint] = glyph_idx as u16;
                            }
                        }
                        token_start = token_end + 1;
                    }
                }
                pos = glyph_end + 1;
            }
        }

        Some(Font {
            glyphs: &data[headersize..end],
            unicode_table,
            num_glyphs,
            charsize,
            width,
            height,
            bytes_per_row,
            ascii_glyphs,
        })
    }

    fn glyph(&self, ch: char) -> &'static [u8] {
        let codepoint = ch as usize;
        let mut idx =
            if codepoint < self.ascii_glyphs.len() && self.ascii_glyphs[codepoint] != u16::MAX {
                self.ascii_glyphs[codepoint] as usize
            } else {
                codepoint
            };
        if codepoint >= self.ascii_glyphs.len() && !self.unicode_table.is_empty() {
            let mut encoded_buf = [0; 4];
            let encoded = ch.encode_utf8(&mut encoded_buf);
            let mut start = 0;
            for glyph_idx in 0..self.num_glyphs {
                let Some(relative_end) =
                    self.unicode_table[start..].iter().position(|&b| b == 0xff)
                else {
                    break;
                };
                let end = start + relative_end;
                if self.unicode_table.get(start..end) == Some(encoded.as_bytes()) {
                    idx = glyph_idx;
                    break;
                }
                start = end + 1;
            }
        }
        if idx >= self.num_glyphs {
            idx = '?' as usize;
        }
        if idx >= self.num_glyphs {
            idx = 0;
        }
        let glyphs: &'static [u8] = self.glyphs;
        &glyphs[idx * self.charsize..(idx + 1) * self.charsize]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EscState {
    Normal,
    Escape,
    Csi,
}

pub struct Console {
    addr: *mut u8,
    shadow: Vec<u8>,
    graphics_shadow: Option<Vec<u8>>,
    graphics_owner: Option<u64>,
    graphics_dirty: Option<(usize, usize, usize, usize)>,
    presented_cursor: Option<(i32, i32)>,
    width: usize,
    height: usize,
    pitch: usize,
    bytes_per_pixel: usize,
    dirty: Option<(usize, usize, usize, usize)>,
    batch_depth: usize,
    font: Font,
    cols: usize,
    rows: usize,
    cursor_col: usize,
    cursor_row: usize,

    fg: u32,
    bg: u32,
    bold: bool,

    esc: EscState,
    params: [u32; 8],
    nparams: usize,
    cur: u32,
    have_cur: bool,
}

unsafe impl Send for Console {}

impl Console {
    fn new(fb: &FbInfo, font: Font) -> Self {
        let width = fb.width as usize;
        let height = fb.height as usize;
        let pitch = fb.pitch as usize;
        Self {
            addr: fb.addr,
            shadow: vec![0; pitch.saturating_mul(height)],
            graphics_shadow: None,
            graphics_owner: None,
            graphics_dirty: None,
            presented_cursor: None,
            width,
            height,
            pitch,
            bytes_per_pixel: (fb.bpp as usize) / 8,
            dirty: None,
            batch_depth: 0,
            font,
            cols: width / font.width,
            rows: height / font.height,
            cursor_col: 0,
            cursor_row: 0,
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
            bold: false,
            esc: EscState::Normal,
            params: [0; 8],
            nparams: 0,
            cur: 0,
            have_cur: false,
        }
    }

    fn fill_rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
        let x_end = x.saturating_add(w).min(self.width);
        let y_end = y.saturating_add(h).min(self.height);
        if x >= x_end || y >= y_end {
            return;
        }
        let bpp = self.bytes_per_pixel;
        let pixel = [
            color as u8,
            (color >> 8) as u8,
            (color >> 16) as u8,
            (color >> 24) as u8,
        ];
        for py in y..y_end {
            let start = py * self.pitch + x * bpp;
            let row = &mut self.shadow[start..start + (x_end - x) * bpp];
            if color == 0 {
                row.fill(0);
            } else {
                for dst in row.chunks_exact_mut(bpp) {
                    dst.copy_from_slice(&pixel[..bpp]);
                }
            }
        }
    }

    
    
    
    fn flush_rect(&self, x: usize, y: usize, w: usize, h: usize) {
        let x_end = x.saturating_add(w).min(self.width);
        let y_end = y.saturating_add(h).min(self.height);
        if x >= x_end || y >= y_end {
            return;
        }
        let row_bytes = (x_end - x) * self.bytes_per_pixel;
        let mut offset = y * self.pitch + x * self.bytes_per_pixel;
        for _ in y..y_end {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    self.shadow.as_ptr().add(offset),
                    self.addr.add(offset),
                    row_bytes,
                );
            }
            offset += self.pitch;
        }
        crate::drivers::virtio_gpu::present_rect(x, y, x_end - x, y_end - y);
    }

    fn mark_dirty(&mut self, x: usize, y: usize, w: usize, h: usize) {
        let x_end = x.saturating_add(w).min(self.width);
        let y_end = y.saturating_add(h).min(self.height);
        if x >= x_end || y >= y_end {
            return;
        }
        self.dirty = Some(match self.dirty {
            Some((left, top, right, bottom)) => {
                (left.min(x), top.min(y), right.max(x_end), bottom.max(y_end))
            }
            None => (x, y, x_end, y_end),
        });
    }

    fn flush_pending(&mut self) {
        
        
        if self.graphics_owner.is_some() {
            return;
        }
        if let Some((x, y, right, bottom)) = self.dirty.take() {
            self.flush_rect(x, y, right - x, bottom - y);
        }
    }

    fn mark_graphics_dirty(&mut self, x: usize, y: usize, w: usize, h: usize) {
        let x_end = x.saturating_add(w).min(self.width);
        let y_end = y.saturating_add(h).min(self.height);
        if x >= x_end || y >= y_end {
            return;
        }
        self.graphics_dirty = Some(match self.graphics_dirty {
            Some((left, top, right, bottom)) => {
                (left.min(x), top.min(y), right.max(x_end), bottom.max(y_end))
            }
            None => (x, y, x_end, y_end),
        });
    }

    
    
    
    pub fn graphics_enter(&mut self, owner: u64) -> Result<(usize, usize), GraphicsError> {
        if self.graphics_owner.is_some() {
            return Err(GraphicsError::Busy);
        }
        self.flush_pending();
        let len = self
            .pitch
            .checked_mul(self.height)
            .ok_or(GraphicsError::OutOfMemory)?;
        let mut surface = Vec::new();
        surface
            .try_reserve_exact(len)
            .map_err(|_| GraphicsError::OutOfMemory)?;
        surface.resize(len, 0);
        self.graphics_shadow = Some(surface);
        self.graphics_owner = Some(owner);
        self.graphics_dirty = Some((0, 0, self.width, self.height));
        self.presented_cursor = None;
        Ok((self.width, self.height))
    }

    pub fn graphics_fill_rect(
        &mut self,
        owner: u64,
        x: i64,
        y: i64,
        width: usize,
        height: usize,
        color: u32,
    ) -> bool {
        if self.graphics_owner != Some(owner) {
            return false;
        }
        let x0 = x.clamp(0, self.width as i64) as usize;
        let y0 = y.clamp(0, self.height as i64) as usize;
        let x1 = x
            .saturating_add(width.min(i64::MAX as usize) as i64)
            .clamp(0, self.width as i64) as usize;
        let y1 = y
            .saturating_add(height.min(i64::MAX as usize) as i64)
            .clamp(0, self.height as i64) as usize;
        if x0 >= x1 || y0 >= y1 {
            return true;
        }

        let bpp = self.bytes_per_pixel;
        let pixel = [color as u8, (color >> 8) as u8, (color >> 16) as u8, 0];
        let surface = self.graphics_shadow.as_mut().unwrap();
        for py in y0..y1 {
            let start = py * self.pitch + x0 * bpp;
            let row = &mut surface[start..start + (x1 - x0) * bpp];
            if bpp == 4 {
                #[cfg(target_arch = "x86_64")]
                if x1 - x0 >= 8 {
                    unsafe {
                        let destination = row.as_mut_ptr();
                        let pixels = x1 - x0;
                        let packed = color & 0x00ff_ffff;
                        core::arch::asm!(
                            "rep stosd",
                            inout("rdi") destination => _,
                            inout("rcx") pixels => _,
                            in("eax") packed,
                            options(nostack, preserves_flags),
                        );
                    }
                    continue;
                }
                for destination in row.chunks_exact_mut(4) {
                    destination.copy_from_slice(&pixel);
                }
            } else {
                for destination in row.chunks_exact_mut(bpp) {
                    destination.copy_from_slice(&pixel[..bpp]);
                }
            }
        }
        self.mark_graphics_dirty(x0, y0, x1 - x0, y1 - y0);
        true
    }

    
    pub fn graphics_image(&mut self, owner: u64, x: i64, y: i64, width: usize, pixels: &[u32]) -> bool {
        if self.graphics_owner != Some(owner) || width == 0 || pixels.len() % width != 0 { return false; }
        let height = pixels.len() / width;
        let x0 = x.clamp(0, self.width as i64) as usize;
        let y0 = y.clamp(0, self.height as i64) as usize;
        let x1 = x.saturating_add(width.min(i64::MAX as usize) as i64).clamp(0, self.width as i64) as usize;
        let y1 = y.saturating_add(height.min(i64::MAX as usize) as i64).clamp(0, self.height as i64) as usize;
        if x0 >= x1 || y0 >= y1 { return true; }
        let bpp = self.bytes_per_pixel;
        let surface = self.graphics_shadow.as_mut().unwrap();
        for py in y0..y1 {
            let src_y = (py as i64 - y) as usize;
            let src_x = (x0 as i64 - x) as usize;
            let row = &pixels[src_y * width + src_x..src_y * width + src_x + (x1 - x0)];
            let mut offset = py * self.pitch + x0 * bpp;
            for &color in row {
                surface[offset..offset + bpp].copy_from_slice(&[color as u8, (color >> 8) as u8, (color >> 16) as u8, 0][..bpp]);
                offset += bpp;
            }
        }
        self.mark_graphics_dirty(x0, y0, x1 - x0, y1 - y0);
        true
    }

    
    
    pub fn graphics_blit_scaled(&mut self, owner: u64, source_width: usize, source_height: usize, pixels: &[u32]) -> bool {
        if self.graphics_owner != Some(owner) || source_width == 0 || source_height == 0
            || source_width.checked_mul(source_height) != Some(pixels.len()) { return false; }
        let screen_w = self.width as u64;
        let screen_h = self.height as u64;
        let (source_w, source_h) = (source_width as u64, source_height as u64);
        if screen_w == 0 || screen_h == 0 { return true; }
        let (dest_w, dest_h) = if screen_w * source_h <= screen_h * source_w {
            (screen_w, (screen_w * source_h / source_w).max(1))
        } else {
            ((screen_h * source_w / source_h).max(1), screen_h)
        };
        let x0 = ((screen_w - dest_w) / 2) as usize;
        let y0 = ((screen_h - dest_h) / 2) as usize;
        let bpp = self.bytes_per_pixel;
        let surface = self.graphics_shadow.as_mut().unwrap();

        
        
        
        
        
        if bpp == 4 && dest_w % source_w == 0 && dest_h % source_h == 0
            && dest_w / source_w == dest_h / source_h
        {
            let scale = (dest_w / source_w) as usize;
            let row_bytes = dest_w as usize * bpp;
            let base = surface.as_mut_ptr();
            for sy in 0..source_height {
                let dest_y = y0 + sy * scale;
                let row_offset = dest_y * self.pitch + x0 * bpp;
                let row = unsafe { base.add(row_offset).cast::<u32>() };
                for sx in 0..source_width {
                    let color = pixels[sy * source_width + sx];
                    for repeat in 0..scale {
                        unsafe { row.add(sx * scale + repeat).write_unaligned(color); }
                    }
                }
                for repeat in 1..scale {
                    let destination = unsafe { base.add((dest_y + repeat) * self.pitch + x0 * bpp) };
                    unsafe { core::ptr::copy_nonoverlapping(row.cast::<u8>(), destination, row_bytes); }
                }
            }
            self.mark_graphics_dirty(x0, y0, dest_w as usize, dest_h as usize);
            return true;
        }

        
        
        
        
        
        
        if bpp == 4 {
            let out_w = dest_w as usize;
            let out_h = dest_h as usize;
            let src_w = source_width;
            let src_h = source_height;
            let x_step = src_w / out_w;
            let x_remainder = src_w % out_w;
            let y_step = src_h / out_h;
            let y_remainder = src_h % out_h;
            let base = surface.as_mut_ptr();
            let mut source_y = 0usize;
            let mut y_error = 0usize;

            for dy in 0..out_h {
                let source_row = &pixels[source_y * src_w..(source_y + 1) * src_w];
                let row_offset = (y0 + dy) * self.pitch + x0 * bpp;
                let row = unsafe { base.add(row_offset).cast::<u32>() };
                let mut source_x = 0usize;
                let mut x_error = 0usize;

                for dx in 0..out_w {
                    unsafe { row.add(dx).write_unaligned(source_row[source_x]); }
                    source_x += x_step;
                    x_error += x_remainder;
                    if x_error >= out_w {
                        source_x += 1;
                        x_error -= out_w;
                    }
                }

                source_y += y_step;
                y_error += y_remainder;
                if y_error >= out_h {
                    source_y += 1;
                    y_error -= out_h;
                }
            }

            self.mark_graphics_dirty(x0, y0, dest_w as usize, dest_h as usize);
            return true;
        }

        for dy in 0..dest_h as usize {
            let sy = (dy as u64 * source_h / dest_h) as usize;
            let mut offset = (y0 + dy) * self.pitch + x0 * bpp;
            for dx in 0..dest_w as usize {
                let sx = (dx as u64 * source_w / dest_w) as usize;
                let color = pixels[sy * source_width + sx];
                if bpp == 4 {
                    unsafe { surface.as_mut_ptr().add(offset).cast::<u32>().write_unaligned(color); }
                } else {
                    surface[offset] = color as u8;
                    surface[offset + 1] = (color >> 8) as u8;
                    surface[offset + 2] = (color >> 16) as u8;
                }
                offset += bpp;
            }
        }
        self.mark_graphics_dirty(x0, y0, dest_w as usize, dest_h as usize);
        true
    }

    
    
    
    pub fn graphics_text(&mut self, owner: u64, x: i64, y: i64, color: u32, text: &str) -> bool {
        if self.graphics_owner != Some(owner) {
            return false;
        }
        let font = self.font;
        let bpp = self.bytes_per_pixel;
        let pixel = [color as u8, (color >> 8) as u8, (color >> 16) as u8, 0];
        let start_x = x;
        let mut pen_x = x;
        let mut pen_y = y;
        let mut dirty: Option<(usize, usize, usize, usize)> = None;
        let surface = self.graphics_shadow.as_mut().unwrap();

        for ch in text.chars() {
            if ch == '\n' {
                pen_x = start_x;
                pen_y = pen_y.saturating_add(font.height as i64);
                continue;
            }
            let glyph = font.glyph(ch);
            for gy in 0..font.height {
                let py = pen_y.saturating_add(gy as i64);
                if py < 0 || py >= self.height as i64 {
                    continue;
                }
                let glyph_row = &glyph[gy * font.bytes_per_row..(gy + 1) * font.bytes_per_row];
                for gx in 0..font.width {
                    let px = pen_x.saturating_add(gx as i64);
                    if px < 0
                        || px >= self.width as i64
                        || glyph_row[gx / 8] & (0x80 >> (gx & 7)) == 0
                    {
                        continue;
                    }
                    let offset = py as usize * self.pitch + px as usize * bpp;
                    surface[offset..offset + bpp].copy_from_slice(&pixel[..bpp]);
                    let (px, py) = (px as usize, py as usize);
                    dirty = Some(match dirty {
                        Some((left, top, right, bottom)) => (
                            left.min(px),
                            top.min(py),
                            right.max(px + 1),
                            bottom.max(py + 1),
                        ),
                        None => (px, py, px + 1, py + 1),
                    });
                }
            }
            pen_x = pen_x.saturating_add(font.width as i64);
        }
        if let Some((left, top, right, bottom)) = dirty {
            self.mark_graphics_dirty(left, top, right - left, bottom - top);
        }
        true
    }

    pub fn graphics_present(&mut self, owner: u64, cursor: (i32, i32)) -> bool {
        if self.graphics_owner != Some(owner) {
            return false;
        }

        let dirty = self.graphics_dirty.take();
        if let Some((left, top, right, bottom)) = dirty {
            self.copy_graphics_rect(left, top, right - left, bottom - top, None);
        }

        let cursor_moved = self.presented_cursor != Some(cursor);
        if cursor_moved {
            if let Some(previous) = self.presented_cursor {
                if !dirty.is_some_and(|rect| Self::rect_intersects_cursor(rect, previous)) {
                    self.copy_cursor_rect(previous.0, previous.1, false);
                }
            }
            self.copy_cursor_rect(cursor.0, cursor.1, true);
            self.presented_cursor = Some(cursor);
        } else if dirty.is_some_and(|rect| Self::rect_intersects_cursor(rect, cursor)) {
            
            
            
            self.copy_cursor_rect(cursor.0, cursor.1, true);
        }
        true
    }

    fn rect_intersects_cursor(rect: (usize, usize, usize, usize), cursor: (i32, i32)) -> bool {
        const CURSOR_W: i32 = 12;
        const CURSOR_H: i32 = 20;
        let (left, top, right, bottom) = rect;
        let cursor_left = cursor.0.max(0) as usize;
        let cursor_top = cursor.1.max(0) as usize;
        let cursor_right = cursor.0.saturating_add(CURSOR_W).max(0) as usize;
        let cursor_bottom = cursor.1.saturating_add(CURSOR_H).max(0) as usize;
        cursor_left < right && cursor_right > left && cursor_top < bottom && cursor_bottom > top
    }

    fn copy_graphics_rect(
        &self,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        cursor: Option<(i32, i32)>,
    ) {
        let x_end = x.saturating_add(w).min(self.width);
        let y_end = y.saturating_add(h).min(self.height);
        if x >= x_end || y >= y_end {
            return;
        }
        let row_bytes = (x_end - x) * self.bytes_per_pixel;
        let mut offset = y * self.pitch + x * self.bytes_per_pixel;
        let surface = self.graphics_shadow.as_ref().unwrap();
        for _ in y..y_end {
            unsafe {
                copy_framebuffer_row(
                    surface.as_ptr().add(offset),
                    self.addr.add(offset),
                    row_bytes,
                    self.bytes_per_pixel,
                );
            }
            offset += self.pitch;
        }
        if let Some((cursor_x, cursor_y)) = cursor {
            self.draw_cursor(cursor_x, cursor_y);
        }
        unsafe { core::arch::asm!("sfence", options(nostack, preserves_flags)) };
        crate::drivers::virtio_gpu::present_rect(x, y, x_end - x, y_end - y);
    }

    fn copy_cursor_rect(&self, x: i32, y: i32, draw_cursor: bool) {
        const CURSOR_W: usize = 12;
        const CURSOR_H: usize = 20;
        let left = x.max(0) as usize;
        let top = y.max(0) as usize;
        if left >= self.width || top >= self.height {
            return;
        }
        let width = CURSOR_W.min(self.width - left);
        let height = CURSOR_H.min(self.height - top);
        self.copy_graphics_rect(left, top, width, height, draw_cursor.then_some((x, y)));
    }

    fn draw_cursor(&self, x: i32, y: i32) {
        const CURSOR_W: i32 = 12;
        const CURSOR_H: i32 = 20;
        let bpp = self.bytes_per_pixel;
        let black = [0x12, 0x16, 0x20, 0];
        let white = [0xF4, 0xF6, 0xFA, 0];
        for cy in 0..CURSOR_H {
            for cx in 0..CURSOR_W {
                let inside = cursor_shape(cx, cy);
                if !inside {
                    continue;
                }
                let edge = !cursor_shape(cx - 1, cy)
                    || !cursor_shape(cx + 1, cy)
                    || !cursor_shape(cx, cy - 1)
                    || !cursor_shape(cx, cy + 1);
                let px = x + cx;
                let py = y + cy;
                if px < 0 || py < 0 || px >= self.width as i32 || py >= self.height as i32 {
                    continue;
                }
                let offset = py as usize * self.pitch + px as usize * bpp;
                let color = if edge { &black } else { &white };
                unsafe {
                    core::ptr::copy_nonoverlapping(color.as_ptr(), self.addr.add(offset), bpp);
                }
            }
        }
    }

    
    pub fn graphics_exit(&mut self, owner: u64) -> bool {
        if self.graphics_owner != Some(owner) {
            return false;
        }
        self.graphics_shadow = None;
        self.graphics_owner = None;
        self.graphics_dirty = None;
        self.presented_cursor = None;
        self.dirty = None;
        self.flush_rect(0, 0, self.width, self.height);
        true
    }

    
    
    pub fn begin_batch(&mut self) {
        self.batch_depth += 1;
    }

    pub fn end_batch(&mut self) {
        self.batch_depth = self.batch_depth.saturating_sub(1);
        if self.batch_depth == 0 {
            self.flush_pending();
        }
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn clear(&mut self) {
        let bg = self.bg;
        self.fill_rect(0, 0, self.width, self.height, bg);
        self.mark_dirty(0, 0, self.width, self.height);
        self.cursor_col = 0;
        self.cursor_row = 0;
    }

    fn draw_glyph(&mut self, ch: char, col: usize, row: usize) {
        let font = self.font;
        let glyph = font.glyph(ch);
        let (fg, bg) = (self.fg, self.bg);
        let ox = col * font.width;
        let oy = row * font.height;

        let bpp = self.bytes_per_pixel;
        let fg_bytes = [
            fg as u8,
            (fg >> 8) as u8,
            (fg >> 16) as u8,
            (fg >> 24) as u8,
        ];
        let bg_bytes = [
            bg as u8,
            (bg >> 8) as u8,
            (bg >> 16) as u8,
            (bg >> 24) as u8,
        ];
        let row_bytes = font.width * bpp;
        let mut dst = oy * self.pitch + ox * bpp;
        for y in 0..font.height {
            let glyph_row = &glyph[y * font.bytes_per_row..(y + 1) * font.bytes_per_row];
            let row = &mut self.shadow[dst..dst + row_bytes];
            for (x, pixel) in row.chunks_exact_mut(bpp).enumerate() {
                let color = if glyph_row[x / 8] & (0x80 >> (x & 7)) != 0 {
                    &fg_bytes
                } else {
                    &bg_bytes
                };
                pixel.copy_from_slice(&color[..bpp]);
            }
            dst += self.pitch;
        }
        self.mark_dirty(ox, oy, font.width, font.height);
    }

    fn newline(&mut self) {
        self.cursor_col = 0;
        if self.cursor_row + 1 >= self.rows {
            self.scroll();
        } else {
            self.cursor_row += 1;
        }
    }

    fn scroll(&mut self) {
        let row_bytes = self.font.height * self.pitch;
        let text_bytes = self.rows * row_bytes;
        self.shadow.copy_within(row_bytes..text_bytes, 0);
        let y = (self.rows - 1) * self.font.height;
        let h = self.font.height;
        self.fill_rect(0, y, self.width, h, DEFAULT_BG);
        self.mark_dirty(0, 0, self.width, text_bytes / self.pitch);
    }

    fn put_printable(&mut self, c: char) {
        if self.cursor_col >= self.cols {
            self.newline();
        }
        self.draw_glyph(c, self.cursor_col, self.cursor_row);
        self.cursor_col += 1;
    }

    fn push_param(&mut self) {
        let v = if self.have_cur { self.cur } else { 0 };
        if self.nparams < self.params.len() {
            self.params[self.nparams] = v;
            self.nparams += 1;
        }
        self.cur = 0;
        self.have_cur = false;
    }

    fn extended_color(&self, i: usize) -> Option<(u32, usize)> {
        let n = self.nparams;
        if i + 4 < n && self.params[i + 1] == 2 {
            let r = self.params[i + 2] & 0xFF;
            let g = self.params[i + 3] & 0xFF;
            let b = self.params[i + 4] & 0xFF;
            return Some(((r << 16) | (g << 8) | b, 4));
        }
        if i + 2 < n && self.params[i + 1] == 5 {
            let idx = self.params[i + 2] as usize;
            if idx < 16 {
                return Some((PALETTE[idx], 2));
            }
            return Some((DEFAULT_FG, 2));
        }
        None
    }

    fn apply_sgr(&mut self) {
        let mut i = 0;
        while i < self.nparams {
            let p = self.params[i];
            match p {
                0 => {
                    self.fg = DEFAULT_FG;
                    self.bg = DEFAULT_BG;
                    self.bold = false;
                }
                1 => self.bold = true,
                22 => self.bold = false,
                30..=37 => {
                    let idx = (p - 30) as usize + if self.bold { 8 } else { 0 };
                    self.fg = PALETTE[idx];
                }
                39 => self.fg = DEFAULT_FG,
                40..=47 => self.bg = PALETTE[(p - 40) as usize],
                49 => self.bg = DEFAULT_BG,
                90..=97 => self.fg = PALETTE[(p - 90) as usize + 8],
                100..=107 => self.bg = PALETTE[(p - 100) as usize + 8],
                38 | 48 => {
                    if let Some((color, used)) = self.extended_color(i) {
                        if p == 38 {
                            self.fg = color;
                        } else {
                            self.bg = color;
                        }
                        i += used;
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    fn put_char(&mut self, c: char) {
        if c == '\x1b' {
            self.esc = EscState::Escape;
            return;
        }

        match self.esc {
            EscState::Escape => {
                if c == '[' {
                    self.esc = EscState::Csi;
                    self.nparams = 0;
                    self.cur = 0;
                    self.have_cur = false;
                } else {
                    self.esc = EscState::Normal;
                }
                return;
            }
            EscState::Csi => {
                match c {
                    '0'..='9' => {
                        self.cur = (self.cur * 10 + (c as u32 - '0' as u32)).min(65535);
                        self.have_cur = true;
                    }
                    ';' => self.push_param(),
                    'm' => {
                        self.push_param();
                        self.apply_sgr();
                        self.esc = EscState::Normal;
                    }
                    'D' => {
                        
                        
                        
                        
                        
                        
                        self.push_param();
                        let n = if self.nparams > 0 {
                            self.params[0] as usize
                        } else {
                            1
                        };
                        self.cursor_left(n);
                        self.esc = EscState::Normal;
                    }
                    'J' => {
                        
                        
                        
                        
                        
                        
                        
                        
                        
                        
                        self.push_param();
                        self.clear();
                        self.esc = EscState::Normal;
                    }
                    _ => self.esc = EscState::Normal,
                }
                return;
            }
            EscState::Normal => {}
        }

        match c {
            '\n' => self.newline(),
            '\r' => self.cursor_col = 0,
            '\x08' => self.backspace(),
            '\t' => {
                let next = (self.cursor_col + 4) & !3;
                while self.cursor_col < next {
                    self.put_printable(' ');
                }
            }
            c => self.put_printable(c),
        }
    }

    
    
    
    
    
    
    
    
    fn backspace(&mut self) {
        if self.cursor_col == 0 {
            return;
        }
        self.cursor_col -= 1;
        self.draw_glyph(' ', self.cursor_col, self.cursor_row);
    }

    
    
    
    
    
    
    pub fn cursor_left(&mut self, n: usize) {
        self.cursor_col = self.cursor_col.saturating_sub(n);
    }
}




unsafe fn copy_framebuffer_row(source: *const u8, destination: *mut u8, len: usize, bpp: usize) {
    #[cfg(target_arch = "x86_64")]
    if bpp == 4 && len >= 64 {
        let qwords = len / 8;
        let copied = qwords * 8;
        core::arch::asm!(
            "rep movsq",
            inout("rsi") source => _,
            inout("rdi") destination => _,
            inout("rcx") qwords => _,
            options(nostack, preserves_flags),
        );
        core::ptr::copy_nonoverlapping(
            source.add(copied),
            destination.add(copied),
            len - copied,
        );
        return;
    }

    core::ptr::copy_nonoverlapping(source, destination, len);
}

impl fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            self.put_char(c);
        }
        if self.batch_depth == 0 {
            self.flush_pending();
        }
        Ok(())
    }
}

fn cursor_shape(x: i32, y: i32) -> bool {
    let arrow_head = (0..=12).contains(&y) && x >= 0 && x <= y / 2 + 2;
    let arrow_tail = (7..=18).contains(&y) && (3..=7).contains(&x);
    arrow_head || arrow_tail
}

pub fn init() {
    let fb_guard = crate::framebuffer::FRAMEBUFFER.lock();
    let Some(fb) = fb_guard.as_ref() else {
        crate::log_fail!("Console", "Init", "No framebuffer available");
        return;
    };

    if fb.bpp != 32 && fb.bpp != 24 {
        crate::log_fail!(
            "Console",
            "Init",
            "Unsupported bpp {} (only 24bpp/32bpp supported)",
            fb.bpp
        );
        return;
    }

    let Some(font) = Font::parse(FONT_DATA) else {
        crate::log_fail!("Console", "Init", "font.psf is not a valid PSF1/PSF2 font");
        return;
    };

    let mut console = Console::new(fb, font);
    if console.cols == 0 || console.rows == 0 {
        crate::log_fail!("Console", "Init", "Screen too small for the selected font");
        return;
    }

    console.clear();
    console.flush_pending();

    let (cols, rows) = (console.cols, console.rows);

    *CONSOLE.lock() = Some(console);
    drop(fb_guard);

    crate::logger::flush_to_console();

    crate::log_ok!(
        "Console",
        "Init",
        "Text console ready ({}x{} chars, {}x{} PSF font)",
        cols,
        rows,
        font.width,
        font.height
    );
}
