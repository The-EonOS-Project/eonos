

















#![allow(dead_code)]

use core::sync::atomic::{AtomicBool, Ordering};

use crate::idt;
use crate::sync::IrqMutex;
use crate::{log_fail, log_ok};

const DATA_PORT: u16 = 0x60;
const STATUS_PORT: u16 = 0x64;
const COMMAND_PORT: u16 = 0x64;

const STATUS_OUTPUT_FULL: u8 = 1 << 0;
const STATUS_INPUT_FULL: u8 = 1 << 1;

const CMD_READ_CONFIG: u8 = 0x20;
const CMD_WRITE_CONFIG: u8 = 0x60;
const CMD_DISABLE_PORT1: u8 = 0xAD;
const CMD_ENABLE_PORT1: u8 = 0xAE;
const CMD_DISABLE_PORT2: u8 = 0xA7;
const CMD_SELF_TEST: u8 = 0xAA;
const CMD_TEST_PORT1: u8 = 0xAB;

const SELF_TEST_OK: u8 = 0x55;
const PORT_TEST_OK: u8 = 0x00;

const CONFIG_PORT1_IRQ: u8 = 1 << 0;
const CONFIG_PORT1_TRANSLATION: u8 = 1 << 6;

const KB_CMD_RESET: u8 = 0xFF;
const KB_RESET_ACK: u8 = 0xFA;
const KB_RESET_PASS: u8 = 0xAA;

const IRQ_LINE: u8 = 1;





const BUFFER_CAP: usize = 256;

struct RingBuffer {
    buf: [u8; BUFFER_CAP],
    head: usize, 
    tail: usize, 
    len: usize,
}

impl RingBuffer {
    const fn new() -> Self {
        RingBuffer {
            buf: [0; BUFFER_CAP],
            head: 0,
            tail: 0,
            len: 0,
        }
    }

    fn push(&mut self, byte: u8) {
        if self.len == BUFFER_CAP {
            
            
            
            self.tail = (self.tail + 1) % BUFFER_CAP;
            self.len -= 1;
        }
        self.buf[self.head] = byte;
        self.head = (self.head + 1) % BUFFER_CAP;
        self.len += 1;
    }

    fn pop(&mut self) -> Option<u8> {
        if self.len == 0 {
            return None;
        }
        let b = self.buf[self.tail];
        self.tail = (self.tail + 1) % BUFFER_CAP;
        self.len -= 1;
        Some(b)
    }
}

static ASCII_QUEUE: IrqMutex<RingBuffer> = IrqMutex::new(RingBuffer::new());
static SHIFT_HELD: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);
static CAPS_LOCK: AtomicBool = AtomicBool::new(false);
static READY: AtomicBool = AtomicBool::new(false);

unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
}

unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    core::arch::asm!("in al, dx", in("dx") port, out("al") val, options(nomem, nostack, preserves_flags));
    val
}




unsafe fn wait_input_clear() -> bool {
    for _ in 0..100_000 {
        if inb(STATUS_PORT) & STATUS_INPUT_FULL == 0 {
            return true;
        }
    }
    false
}


unsafe fn wait_output_full() -> bool {
    for _ in 0..100_000 {
        if inb(STATUS_PORT) & STATUS_OUTPUT_FULL != 0 {
            return true;
        }
    }
    false
}

unsafe fn write_command(cmd: u8) -> bool {
    if !wait_input_clear() {
        return false;
    }
    outb(COMMAND_PORT, cmd);
    true
}

unsafe fn write_data(byte: u8) -> bool {
    if !wait_input_clear() {
        return false;
    }
    outb(DATA_PORT, byte);
    true
}

unsafe fn read_data() -> Option<u8> {
    if !wait_output_full() {
        return None;
    }
    Some(inb(DATA_PORT))
}






pub fn init() {
    unsafe {
        
        
        
        
        if inb(STATUS_PORT) == 0xFF {
            log_ok!(
                "PS2",
                "Init",
                "No PS/2 controller detected; continuing without it"
            );
            return;
        }

        
        
        
        write_command(CMD_DISABLE_PORT1);
        write_command(CMD_DISABLE_PORT2);

        
        
        for _ in 0..100_000 {
            if inb(STATUS_PORT) & STATUS_OUTPUT_FULL == 0 {
                break;
            }
            inb(DATA_PORT);
        }

        if !write_command(CMD_SELF_TEST) {
            log_fail!(
                "PS2",
                "Init",
                "Controller did not accept the self-test command"
            );
            return;
        }
        match read_data() {
            Some(SELF_TEST_OK) => {}
            Some(other) => {
                log_fail!(
                    "PS2",
                    "Init",
                    "Controller self-test failed (response {:#04x})",
                    other
                );
                return;
            }
            None => {
                log_fail!(
                    "PS2",
                    "Init",
                    "Controller self-test timed out -- no PS/2 controller?"
                );
                return;
            }
        }

        if !write_command(CMD_TEST_PORT1) {
            log_fail!(
                "PS2",
                "Init",
                "Controller did not accept the port-1 test command"
            );
            return;
        }
        match read_data() {
            Some(PORT_TEST_OK) => {}
            Some(other) => {
                log_fail!(
                    "PS2",
                    "Init",
                    "Port 1 test failed (response {:#04x})",
                    other
                );
                return;
            }
            None => {
                log_fail!("PS2", "Init", "Port 1 test timed out");
                return;
            }
        }

        
        
        
        if !write_command(CMD_READ_CONFIG) {
            log_fail!(
                "PS2",
                "Init",
                "Controller did not accept read-config command"
            );
            return;
        }
        let Some(mut config) = read_data() else {
            log_fail!("PS2", "Init", "Timed out reading controller configuration");
            return;
        };
        config |= CONFIG_PORT1_IRQ | CONFIG_PORT1_TRANSLATION;
        if !write_command(CMD_WRITE_CONFIG) || !write_data(config) {
            log_fail!(
                "PS2",
                "Init",
                "Failed to write back controller configuration"
            );
            return;
        }

        if !write_command(CMD_ENABLE_PORT1) {
            log_fail!("PS2", "Init", "Failed to enable port 1");
            return;
        }

        
        
        
        if !write_data(KB_CMD_RESET) {
            log_fail!("PS2", "Init", "Failed to send reset to keyboard");
            return;
        }
        let mut saw_ack = false;
        let mut saw_pass = false;
        for _ in 0..2 {
            match read_data() {
                Some(KB_RESET_ACK) => saw_ack = true,
                Some(KB_RESET_PASS) => saw_pass = true,
                Some(_) => {} 
                None => break,
            }
        }
        if !saw_ack && !saw_pass {
            log_fail!(
                "PS2",
                "Init",
                "Keyboard did not respond to reset -- no keyboard attached?"
            );
            return;
        }
    }

    idt::register_irq(IRQ_LINE, on_key_irq);
    crate::interrupts::unmask_isa_irq(IRQ_LINE);
    READY.store(true, Ordering::Release);

    log_ok!(
        "PS2",
        "Init",
        "Controller and keyboard ready, IRQ1 registered and unmasked"
    );
}




pub fn is_ready() -> bool {
    READY.load(Ordering::Acquire)
}

fn on_key_irq() {
    
    
    let scancode = unsafe { inb(DATA_PORT) };
    handle_scancode(scancode);
}




#[rustfmt::skip]
const SET1_UNSHIFTED: [u8; 0x60] = [
    0,    0x1B, b'1', b'2', b'3', b'4', b'5', b'6', 
    b'7', b'8', b'9', b'0', b'-', b'=', 0x08, b'\t', 
    b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i', 
    b'o', b'p', b'[', b']', b'\n', 0,    b'a', b's', 
    b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';', 
    b'\'', b'`', 0,    b'\\', b'z', b'x', b'c', b'v', 
    b'b', b'n', b'm', b',', b'.', b'/', 0,    b'*', 
    0,    b' ', 0,    0,    0,    0,    0,    0,    
    0,    0,    0,    0,    0,    0,    0,    b'7', 
    b'8', b'9', b'-', b'4', b'5', b'6', b'+', b'1', 
    b'2', b'3', b'0', b'.', 0,    0,    0,    0,    
    0,    0,    0,    0,    0,    0,    0,    0,    
];

#[rustfmt::skip]
const SET1_SHIFTED: [u8; 0x60] = [
    0,    0x1B, b'!', b'@', b'#', b'$', b'%', b'^', 
    b'&', b'*', b'(', b')', b'_', b'+', 0x08, b'\t', 
    b'Q', b'W', b'E', b'R', b'T', b'Y', b'U', b'I', 
    b'O', b'P', b'{', b'}', b'\n', 0,    b'A', b'S', 
    b'D', b'F', b'G', b'H', b'J', b'K', b'L', b':', 
    b'"', b'~', 0,    b'|', b'Z', b'X', b'C', b'V', 
    b'B', b'N', b'M', b'<', b'>', b'?', 0,    b'*', 
    0,    b' ', 0,    0,    0,    0,    0,    0,    
    0,    0,    0,    0,    0,    0,    0,    b'7', 
    b'8', b'9', b'-', b'4', b'5', b'6', b'+', b'1', 
    b'2', b'3', b'0', b'.', 0,    0,    0,    0,    
    0,    0,    0,    0,    0,    0,    0,    0,    
];

const SCANCODE_LSHIFT: u8 = 0x2A;
const SCANCODE_RSHIFT: u8 = 0x36;
const SCANCODE_LCTRL: u8 = 0x1D;
const SCANCODE_CAPS_LOCK: u8 = 0x3A;






const BREAK_BIT: u8 = 0x80;
const EXTENDED_PREFIX: u8 = 0xE0;

static EXPECT_EXTENDED: AtomicBool = AtomicBool::new(false);
static CTRL_HELD: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);
static ALT_HELD: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);


const EXT_UP: u8 = 0x48;
const EXT_DOWN: u8 = 0x50;
const EXT_RIGHT: u8 = 0x4D;
const EXT_LEFT: u8 = 0x4B;
const EXT_HOME: u8 = 0x47;
const EXT_END: u8 = 0x4F;
const EXT_DELETE: u8 = 0x53;
const EXT_RCTRL: u8 = 0x1D;

fn set_modifier(state: &core::sync::atomic::AtomicU8, side: u8, pressed: bool) {
    if pressed { state.fetch_or(side, Ordering::Relaxed); }
    else { state.fetch_and(!side, Ordering::Relaxed); }
}

fn hid_usage(code: u8, extended: bool) -> Option<u8> {
    if extended {
        return Some(match code {
            EXT_RCTRL => 0xE4, 0x38 => 0xE6, 0x5B => 0xE3, 0x5C => 0xE7,
            0x1C => 0x58, 0x35 => 0x54,
            EXT_HOME => 0x4A, 0x4F => 0x4D, 0x49 => 0x4B,
            0x51 => 0x4E, 0x52 => 0x49, EXT_DELETE => 0x4C,
            EXT_UP => 0x52, EXT_DOWN => 0x51, EXT_RIGHT => 0x4F, EXT_LEFT => 0x50,
            _ => return None,
        });
    }
    Some(match code {
        0x01 => 0x29,
        0x02..=0x0A => 0x1E + code - 0x02,
        0x0B => 0x27, 0x0C => 0x2D, 0x0D => 0x2E, 0x0E => 0x2A, 0x0F => 0x2B,
        0x10 => 0x14, 0x11 => 0x1A, 0x12 => 0x08, 0x13 => 0x15, 0x14 => 0x17,
        0x15 => 0x1C, 0x16 => 0x18, 0x17 => 0x0C, 0x18 => 0x12, 0x19 => 0x13,
        0x1A => 0x2F, 0x1B => 0x30, 0x1C => 0x28, 0x1D => 0xE0,
        0x1E => 0x04, 0x1F => 0x16, 0x20 => 0x07, 0x21 => 0x09, 0x22 => 0x0A,
        0x23 => 0x0B, 0x24 => 0x0D, 0x25 => 0x0E, 0x26 => 0x0F, 0x27 => 0x33,
        0x28 => 0x34, 0x29 => 0x35, 0x2A => 0xE1, 0x2B => 0x31,
        0x2C => 0x1D, 0x2D => 0x1B, 0x2E => 0x06, 0x2F => 0x19,
        0x30 => 0x05, 0x31 => 0x11, 0x32 => 0x10, 0x33 => 0x36, 0x34 => 0x37,
        0x35 => 0x38, 0x36 => 0xE5, 0x37 => 0x55, 0x38 => 0xE2, 0x39 => 0x2C,
        0x3A => 0x39, 0x3B..=0x44 => 0x3A + code - 0x3B, 0x45 => 0x53,
        0x47 => 0x5F, 0x48 => 0x60, 0x49 => 0x61, 0x4A => 0x56, 0x4B => 0x5C,
        0x4C => 0x5D, 0x4D => 0x5E, 0x4E => 0x57, 0x4F => 0x59, 0x50 => 0x5A,
        0x51 => 0x5B, 0x52 => 0x62, 0x53 => 0x63, 0x57 => 0x44, 0x58 => 0x45,
        _ => return None,
    })
}

fn modifier_mask() -> u8 {
    let mut bits = 0;
    let shift = SHIFT_HELD.load(Ordering::Relaxed);
    let ctrl = CTRL_HELD.load(Ordering::Relaxed);
    let alt = ALT_HELD.load(Ordering::Relaxed);
    if shift & 1 != 0 { bits |= 1 << 1; }
    if shift & 2 != 0 { bits |= 1 << 5; }
    if ctrl & 1 != 0 { bits |= 1; }
    if ctrl & 2 != 0 { bits |= 1 << 4; }
    if alt & 1 != 0 { bits |= 1 << 2; }
    if alt & 2 != 0 { bits |= 1 << 6; }
    bits
}






fn push_escape(seq: &[u8]) {
    let mut q = ASCII_QUEUE.lock();
    for &b in seq {
        q.push(b);
    }
}

fn handle_scancode(code: u8) {
    if code == EXTENDED_PREFIX {
        EXPECT_EXTENDED.store(true, Ordering::Relaxed);
        return;
    }

    let is_break = code & BREAK_BIT != 0;
    let make_code = code & !BREAK_BIT;
    let extended = EXPECT_EXTENDED.swap(false, Ordering::Relaxed);

    let usage = hid_usage(make_code, extended);
    match usage {
        Some(0xE1) => set_modifier(&SHIFT_HELD, 1, !is_break),
        Some(0xE5) => set_modifier(&SHIFT_HELD, 2, !is_break),
        Some(0xE0) => set_modifier(&CTRL_HELD, 1, !is_break),
        Some(0xE4) => set_modifier(&CTRL_HELD, 2, !is_break),
        Some(0xE2) => set_modifier(&ALT_HELD, 1, !is_break),
        Some(0xE6) => set_modifier(&ALT_HELD, 2, !is_break),
        Some(0x39) if !is_break => CAPS_LOCK.store(!CAPS_LOCK.load(Ordering::Relaxed), Ordering::Relaxed),
        _ => {}
    }
    let focused = usage
        .map(|usage| crate::input::push(usage, !is_break, modifier_mask()))
        .unwrap_or(false);

    if extended {
        if make_code == EXT_RCTRL {
            return;
        }
        if is_break {
            return; 
        }
        if focused { return; }
        match make_code {
            EXT_UP => push_escape(b"\x1b[A"),
            EXT_DOWN => push_escape(b"\x1b[B"),
            EXT_RIGHT => push_escape(b"\x1b[C"),
            EXT_LEFT => push_escape(b"\x1b[D"),
            EXT_HOME => push_escape(b"\x1b[H"),
            EXT_END => push_escape(b"\x1b[F"),
            EXT_DELETE => push_escape(b"\x1b[3~"),
            _ => {} 
        }
        return;
    }

    match make_code {
        SCANCODE_LSHIFT | SCANCODE_RSHIFT => {
            return;
        }
        SCANCODE_LCTRL => {
            return;
        }
        SCANCODE_CAPS_LOCK => {
            return;
        }
        _ => {}
    }

    if is_break {
        return; 
    }
    if focused { return; }

    if (make_code as usize) >= SET1_UNSHIFTED.len() {
        return;
    }

    let shift = SHIFT_HELD.load(Ordering::Relaxed) != 0;
    let caps = CAPS_LOCK.load(Ordering::Relaxed);
    let mut ascii = if shift {
        SET1_SHIFTED[make_code as usize]
    } else {
        SET1_UNSHIFTED[make_code as usize]
    };

    if ascii == 0 {
        return;
    }

    
    
    
    
    if caps && ascii.is_ascii_lowercase() {
        ascii = ascii.to_ascii_uppercase();
    } else if caps && ascii.is_ascii_uppercase() && shift {
        ascii = ascii.to_ascii_lowercase();
    }

    if CTRL_HELD.load(Ordering::Relaxed) != 0 && ascii.is_ascii_alphabetic() {
        ascii = ascii.to_ascii_uppercase() & 0x1F;
    }

    ASCII_QUEUE.lock().push(ascii);
}




pub fn try_read_byte() -> Option<u8> {
    ASCII_QUEUE.lock().pop()
}
