use limine::request::FramebufferRequest;
use spin::Mutex;

use crate::{log_fail, log_ok};

#[used]
#[link_section = ".requests"]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

pub struct FbInfo {
    pub addr: *mut u8,
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub bpp: u16,
}

unsafe impl Send for FbInfo {}

pub static FRAMEBUFFER: Mutex<Option<FbInfo>> = Mutex::new(None);

pub fn use_gpu_backing(addr: *mut u8, width: u64, height: u64, pitch: u64) {
    *FRAMEBUFFER.lock() = Some(FbInfo {
        addr,
        width,
        height,
        pitch,
        bpp: 32,
    });
}

pub fn init() {
    let Some(response) = FRAMEBUFFER_REQUEST.get_response() else {
        log_fail!("Framebuffer", "Init", "No response from bootloader");
        return;
    };

    let Some(fb) = response.framebuffers().next() else {
        log_fail!("Framebuffer", "Init", "No framebuffers reported");
        return;
    };

    let info = FbInfo {
        addr: fb.addr(),
        width: fb.width(),
        height: fb.height(),
        pitch: fb.pitch(),
        bpp: fb.bpp(),
    };

    log_ok!(
        "Framebuffer",
        "Init",
        "Framebuffer Initialized ({}x{} @ {}bpp)",
        info.width,
        info.height,
        info.bpp
    );

    *FRAMEBUFFER.lock() = Some(info);
}

pub fn self_test() {
    let guard = FRAMEBUFFER.lock();
    let Some(fb) = guard.as_ref() else {
        log_fail!("Framebuffer", "SelfTest", "Framebuffer not initialized");
        return;
    };

    if fb.width < 8 || fb.height < 8 || (fb.bpp != 24 && fb.bpp != 32) {
        log_fail!(
            "Framebuffer",
            "SelfTest",
            "Framebuffer is too small or has unsupported pixel format"
        );
        return;
    }

    let bytes_per_pixel = (fb.bpp as usize) / 8;
    let mut original = [0u32; 64];

    unsafe {
        let mut row = fb.addr;
        for y in 0..8 {
            let mut ptr = row;
            for x in 0..8 {
                let i = y * 8 + x;
                original[i] = ptr.read_volatile() as u32
                    | ((ptr.add(1).read_volatile() as u32) << 8)
                    | ((ptr.add(2).read_volatile() as u32) << 16);
                ptr.write_volatile(0xFF);
                ptr.add(1).write_volatile(0xFF);
                ptr.add(2).write_volatile(0xFF);
                ptr = ptr.add(bytes_per_pixel);
            }
            row = row.add(fb.pitch as usize);
        }

        row = fb.addr;
        for y in 0..8 {
            let mut ptr = row;
            for x in 0..8 {
                let rgb = original[y * 8 + x];
                ptr.write_volatile(rgb as u8);
                ptr.add(1).write_volatile((rgb >> 8) as u8);
                ptr.add(2).write_volatile((rgb >> 16) as u8);
                ptr = ptr.add(bytes_per_pixel);
            }
            row = row.add(fb.pitch as usize);
        }
    }

    crate::log_ok!(
        "Framebuffer",
        "SelfTest",
        "Self-test block drawn successfully"
    );
}

unsafe fn rdtsc() -> u64 {
    let lo: u32;
    let hi: u32;
    core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nomem, nostack));
    ((hi as u64) << 32) | lo as u64
}

pub fn bench(label: &str) {
    let guard = FRAMEBUFFER.lock();
    let Some(fb) = guard.as_ref() else {
        log_fail!("Framebuffer", "Bench", "Framebuffer not initialized");
        return;
    };

    let bytes_per_pixel = (fb.bpp as usize) / 8;
    let width = fb.width as usize;
    let height = fb.height as usize;
    if (fb.bpp != 24 && fb.bpp != 32) || width == 0 || height == 0 {
        log_fail!(
            "Framebuffer",
            "Bench",
            "Unsupported framebuffer format or dimensions"
        );
        return;
    }
    let row_bytes = width.saturating_mul(bytes_per_pixel);
    if (fb.pitch as usize) < row_bytes {
        log_fail!(
            "Framebuffer",
            "Bench",
            "Framebuffer pitch is smaller than a pixel row"
        );
        return;
    }
    const N: usize = 4096;

    let y0 = 16.min(height.saturating_sub(1));
    let writes = N.min(height.saturating_sub(y0).saturating_mul(width));
    let mut original = [0u32; N];

    unsafe {

        let mut offset = y0 * fb.pitch as usize;
        let mut x = 0usize;
        for i in 0..writes {
            let ptr = fb.addr.add(offset);
            original[i] = ptr.read_volatile() as u32
                | ((ptr.add(1).read_volatile() as u32) << 8)
                | ((ptr.add(2).read_volatile() as u32) << 16);
            offset += bytes_per_pixel;
            x += 1;
            if x == width {
                x = 0;
                offset += fb.pitch as usize - row_bytes;
            }
        }

        let start = rdtsc();
        offset = y0 * fb.pitch as usize;
        x = 0;
        for _ in 0..writes {
            let ptr = fb.addr.add(offset);
            ptr.write_volatile(0x55);
            ptr.add(1).write_volatile(0x55);
            ptr.add(2).write_volatile(0x55);
            offset += bytes_per_pixel;
            x += 1;
            if x == width {
                x = 0;
                offset += fb.pitch as usize - row_bytes;
            }
        }
        let end = rdtsc();
        let cycles = end - start;

        offset = y0 * fb.pitch as usize;
        x = 0;
        for i in 0..writes {
            let ptr = fb.addr.add(offset);
            let rgb = original[i];
            ptr.write_volatile(rgb as u8);
            ptr.add(1).write_volatile((rgb >> 8) as u8);
            ptr.add(2).write_volatile((rgb >> 16) as u8);
            offset += bytes_per_pixel;
            x += 1;
            if x == width {
                x = 0;
                offset += fb.pitch as usize - row_bytes;
            }
        }

        crate::log_ok!(
            "Framebuffer",
            "Bench",
            "{}: {} pixel writes in {} cycles ({} cycles/pixel)",
            label,
            writes,
            cycles,
            cycles / writes.max(1) as u64
        );

        if bytes_per_pixel == 4 {
            core::arch::asm!("sfence", options(nostack, preserves_flags));
            let start = rdtsc();
            let source = original.as_ptr().cast::<u8>();
            let mut copied = 0usize;
            while copied < writes {
                let x = copied % width;
                let y = y0 + copied / width;
                let count = (width - x).min(writes - copied);
                let destination = fb.addr.add(y * fb.pitch as usize + x * 4);
                core::ptr::copy_nonoverlapping(source.add(copied * 4), destination, count * 4);
                copied += count;
            }
            core::arch::asm!("sfence", options(nostack, preserves_flags));
            let cycles = rdtsc() - start;
            crate::log_ok!(
                "Framebuffer",
                "Bench",
                "{}: bulk row copy of {} pixels in {} cycles ({} cycles/pixel)",
                label,
                writes,
                cycles,
                cycles / writes.max(1) as u64
            );
        }
    }
}
