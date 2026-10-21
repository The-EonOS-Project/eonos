




use super::{checked, syscall3, syscall6, Error, Result};

const SYS_GRAPHICS: usize = 9021;
const ENTER: usize = 0;
const FILL_RECT: usize = 1;
const TEXT: usize = 2;
const PRESENT: usize = 3;
const EXIT: usize = 4;
const IMAGE: usize = 5;
const BLIT_SCALED: usize = 6;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DisplayInfo { pub width: u32, pub height: u32 }


pub struct Surface { info: DisplayInfo, active: bool }

impl Surface {
    
    pub fn open() -> Result<Self> {
        let mut info = DisplayInfo::default();
        checked(unsafe { syscall3(SYS_GRAPHICS, ENTER, (&mut info as *mut DisplayInfo) as usize, 0) })?;
        Ok(Self { info, active: true })
    }

    pub fn dimensions(&self) -> (u32, u32) { (self.info.width, self.info.height) }

    
    pub fn fill_rect(&mut self, x: i32, y: i32, width: u32, height: u32, color: u32) -> Result<()> {
        checked(unsafe { syscall6(SYS_GRAPHICS, FILL_RECT, x as isize as usize, y as isize as usize,
            width as usize, height as usize, color as usize) }).map(|_| ())
    }

    pub fn pixel(&mut self, x: i32, y: i32, color: u32) -> Result<()> {
        self.fill_rect(x, y, 1, 1, color)
    }

    
    
    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: u32) -> Result<()> {
        let (width, height) = (self.info.width as i64, self.info.height as i64);
        if width == 0 || height == 0 { return Ok(()); }
        let (mut ax, mut ay, mut bx, mut by) = (x0 as i64, y0 as i64, x1 as i64, y1 as i64);
        let code = |x: i64, y: i64| -> u8 {
            (if x < 0 { 1 } else if x >= width { 2 } else { 0 })
                | (if y < 0 { 4 } else if y >= height { 8 } else { 0 })
        };
        let intersect = |base: i64, delta: i64, numerator: i64, denominator: i64| -> i64 {
            (base as i128 + delta as i128 * numerator as i128 / denominator as i128) as i64
        };
        loop {
            let (ca, cb) = (code(ax, ay), code(bx, by));
            if ca | cb == 0 { break; }
            if ca & cb != 0 { return Ok(()); }
            let outside = if ca != 0 { ca } else { cb };
            let (nx, ny) = if outside & 8 != 0 {
                if by == ay { return Ok(()); }
                (intersect(ax, bx - ax, height - 1 - ay, by - ay), height - 1)
            } else if outside & 4 != 0 {
                if by == ay { return Ok(()); }
                (intersect(ax, bx - ax, -ay, by - ay), 0)
            } else if outside & 2 != 0 {
                if bx == ax { return Ok(()); }
                (width - 1, intersect(ay, by - ay, width - 1 - ax, bx - ax))
            } else {
                if bx == ax { return Ok(()); }
                (0, intersect(ay, by - ay, -ax, bx - ax))
            };
            if outside == ca { (ax, ay) = (nx, ny); } else { (bx, by) = (nx, ny); }
        }
        let (dx, sx) = ((bx - ax).abs(), if ax < bx { 1 } else { -1 });
        let (dy, sy) = (-(by - ay).abs(), if ay < by { 1 } else { -1 });
        let mut error = dx + dy;
        loop {
            self.pixel(ax as i32, ay as i32, color)?;
            if ax == bx && ay == by { break; }
            let twice = 2 * error;
            if twice >= dy { error += dy; ax += sx; }
            if twice <= dx { error += dx; ay += sy; }
        }
        Ok(())
    }

    
    
    pub fn circle(&mut self, cx: i32, cy: i32, radius: u32, color: u32) -> Result<()> {
        let limit = self.info.width.max(self.info.height).saturating_mul(2);
        if radius > limit { return Err(Error(22)); }
        let (mut x, mut y, mut error) = (radius as i64, 0i64, 1i64 - radius as i64);
        while x >= y {
            for (px, py) in [(x,y),(y,x),(-y,x),(-x,y),(-x,-y),(-y,-x),(y,-x),(x,-y)] {
                let px = cx as i64 + px; let py = cy as i64 + py;
                if px >= 0 && py >= 0 && px < self.info.width as i64 && py < self.info.height as i64 {
                    self.pixel(px as i32, py as i32, color)?;
                }
            }
            y += 1;
            if error < 0 { error += 2 * y + 1; }
            else { x -= 1; error += 2 * (y - x) + 1; }
        }
        Ok(())
    }

    
    pub fn text(&mut self, x: i32, y: i32, color: u32, text: &str) -> Result<()> {
        checked(unsafe { syscall6(SYS_GRAPHICS, TEXT, x as isize as usize, y as isize as usize,
            color as usize, text.as_ptr() as usize, text.len()) }).map(|_| ())
    }

    
    
    pub fn image(&mut self, x: i32, y: i32, width: u32, pixels: &[u32]) -> Result<()> {
        if width == 0 || pixels.is_empty() || pixels.len() % width as usize != 0
            || pixels.len() > 262_144 { return Err(Error(22)); }
        let bytes = unsafe { core::slice::from_raw_parts(pixels.as_ptr() as *const u8, core::mem::size_of_val(pixels)) };
        checked(unsafe { syscall6(SYS_GRAPHICS, IMAGE, x as isize as usize, y as isize as usize,
            width as usize, bytes.as_ptr() as usize, bytes.len()) }).map(|_| ())
    }

    
    
    pub fn blit_scaled(&mut self, width: u32, height: u32, pixels: &[u32]) -> Result<()> {
        let count = (width as usize).checked_mul(height as usize).ok_or(Error(22))?;
        if width == 0 || height == 0 || pixels.len() != count || count > 262_144 {
            return Err(Error(22));
        }
        let bytes = unsafe { core::slice::from_raw_parts(pixels.as_ptr() as *const u8, core::mem::size_of_val(pixels)) };
        checked(unsafe { syscall6(SYS_GRAPHICS, BLIT_SCALED, width as usize, height as usize,
            bytes.as_ptr() as usize, bytes.len(), 0) }).map(|_| ())
    }

    pub fn present(&mut self) -> Result<()> {
        checked(unsafe { syscall3(SYS_GRAPHICS, PRESENT, 0, 0) }).map(|_| ())
    }

    
    pub fn present_without_cursor(&mut self) -> Result<()> {
        checked(unsafe { syscall3(SYS_GRAPHICS, PRESENT, 1, 0) }).map(|_| ())
    }

    
    pub fn close(mut self) -> Result<()> {
        checked(unsafe { syscall3(SYS_GRAPHICS, EXIT, 0, 0) })?;
        self.active = false;
        Ok(())
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        if self.active { let _ = unsafe { syscall3(SYS_GRAPHICS, EXIT, 0, 0) }; }
    }
}
