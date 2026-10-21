

use core::sync::atomic::{AtomicU64, Ordering};

use crate::{log_fail, log_ok};

const INDEX: u16 = 0x70;
const DATA: u16 = 0x71;
const SECOND: u8 = 0x00;
const MINUTE: u8 = 0x02;
const HOUR: u8 = 0x04;
const DAY: u8 = 0x07;
const MONTH: u8 = 0x08;
const YEAR: u8 = 0x09;
const STATUS_A: u8 = 0x0a;
const STATUS_B: u8 = 0x0b;
const CENTURY: u8 = 0x32;

static BASE_UNIX_SECONDS: AtomicU64 = AtomicU64::new(0);
static BASE_UPTIME_MS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Eq, PartialEq)]
struct DateTime {
    second: u8,
    minute: u8,
    hour: u8,
    day: u8,
    month: u8,
    year: u16,
}

unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    core::arch::asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    value
}

fn read_register(register: u8) -> u8 {
    unsafe {
        outb(INDEX, register);
        inb(DATA)
    }
}

fn snapshot() -> Option<DateTime> {
    
    
    let flags: u64;
    unsafe {
        core::arch::asm!("pushfq", "pop {}", "cli", out(reg) flags, options(nomem));
    }
    let result = (|| {
        
        
        for _ in 0..8 {
            if read_register(STATUS_A) & 0x80 != 0 {
                continue;
            }
            let first = read_fields();
            if read_register(STATUS_A) & 0x80 != 0 {
                continue;
            }
            let second = read_fields();
            if first == second {
                return decode(second, read_register(STATUS_B));
            }
        }
        None
    })();
    if flags & (1 << 9) != 0 {
        unsafe {
            core::arch::asm!("sti", options(nomem, nostack));
        }
    }
    result
}

fn read_fields() -> DateTime {
    DateTime {
        second: read_register(SECOND),
        minute: read_register(MINUTE),
        hour: read_register(HOUR),
        day: read_register(DAY),
        month: read_register(MONTH),
        year: ((read_register(CENTURY) as u16) << 8) | read_register(YEAR) as u16,
    }
}

fn decode(mut value: DateTime, status_b: u8) -> Option<DateTime> {
    let binary = status_b & 0x04 != 0;
    let mode_24h = status_b & 0x02 != 0;
    let pm = value.hour & 0x80 != 0;
    value.hour &= 0x7f;
    if !binary {
        value.second = bcd(value.second)?;
        value.minute = bcd(value.minute)?;
        value.hour = bcd(value.hour)?;
        value.day = bcd(value.day)?;
        value.month = bcd(value.month)?;
        let century = (value.year >> 8) as u8;
        let century = if century == 0 { 0 } else { bcd(century)? };
        value.year = ((century as u16) << 8) | bcd(value.year as u8)? as u16;
    }
    let year_in_century = value.year as u8 as u16;
    let century = (value.year >> 8) as u8 as u16;
    if century > 99 {
        return None;
    }
    value.year = if century == 0 {
        year_in_century
    } else {
        century * 100 + year_in_century
    };
    if !mode_24h {
        if value.hour == 0 || value.hour > 12 {
            return None;
        }
        value.hour = (value.hour % 12) + if pm { 12 } else { 0 };
    }
    let year = if value.year >= 100 {
        value.year
    } else if value.year >= 70 {
        1900 + value.year
    } else {
        2000 + value.year
    };
    value.year = year;
    if value.second > 59
        || value.minute > 59
        || value.hour > 23
        || value.month == 0
        || value.month > 12
        || value.day == 0
        || value.day > days_in_month(value.year, value.month)
    {
        return None;
    }
    Some(value)
}

fn bcd(value: u8) -> Option<u8> {
    let lo = value & 0x0f;
    let hi = value >> 4;
    if lo > 9 || hi > 9 {
        None
    } else {
        Some(hi * 10 + lo)
    }
}

fn leap(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}
fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn to_unix_seconds(date: DateTime) -> u64 {
    let mut days = 0u64;
    for year in 1970..date.year {
        days += if leap(year) { 366 } else { 365 };
    }
    for month in 1..date.month {
        days += days_in_month(date.year, month) as u64;
    }
    days += date.day as u64 - 1;
    days * 86_400 + date.hour as u64 * 3_600 + date.minute as u64 * 60 + date.second as u64
}


pub fn init() {
    match snapshot() {
        Some(date) if date.year >= 1970 => {
            BASE_UNIX_SECONDS.store(to_unix_seconds(date), Ordering::Release);
            BASE_UPTIME_MS.store(crate::pit::uptime_ms(), Ordering::Release);
            log_ok!(
                "RTC",
                "Init",
                "UTC clock initialized: {:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                date.year,
                date.month,
                date.day,
                date.hour,
                date.minute,
                date.second
            );
        }
        _ => log_fail!(
            "RTC",
            "Init",
            "No valid CMOS clock reading; wall time unavailable"
        ),
    }
}


pub fn unix_seconds() -> Option<u64> {
    let base = BASE_UNIX_SECONDS.load(Ordering::Acquire);
    if base == 0 {
        return None;
    }
    let elapsed =
        crate::pit::uptime_ms().saturating_sub(BASE_UPTIME_MS.load(Ordering::Acquire)) / 1000;
    Some(base.saturating_add(elapsed))
}
