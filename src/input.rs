use crate::sync::IrqMutex;

const QUEUE_CAPACITY: usize = 512;

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct KeyEvent {
    pub usage: u8,
    pub pressed: u8,
    pub modifiers: u8,
    pub _reserved: u8,
}

struct EventQueue {
    events: [KeyEvent; QUEUE_CAPACITY],
    head: usize,
    tail: usize,
    len: usize,
    owner: Option<u64>,
    held: [u64; 4],
}

impl EventQueue {
    const fn new() -> Self {
        Self { events: [KeyEvent { usage: 0, pressed: 0, modifiers: 0, _reserved: 0 }; QUEUE_CAPACITY],
            head: 0, tail: 0, len: 0, owner: None, held: [0; 4] }
    }

    fn clear_events(&mut self) { self.head = 0; self.tail = 0; self.len = 0; }

    fn push(&mut self, event: KeyEvent) {
        if self.len == QUEUE_CAPACITY { return; }
        self.events[self.head] = event;
        self.head = (self.head + 1) % QUEUE_CAPACITY;
        self.len += 1;
    }

    fn pop(&mut self) -> Option<KeyEvent> {
        if self.len == 0 { return None; }
        let event = self.events[self.tail];
        self.tail = (self.tail + 1) % QUEUE_CAPACITY;
        self.len -= 1;
        Some(event)
    }
}

static EVENTS: IrqMutex<EventQueue> = IrqMutex::new(EventQueue::new());

pub fn claim(pid: u64) -> bool {
    let mut queue = EVENTS.lock();
    if queue.owner.is_some_and(|owner| owner != pid) { return false; }
    queue.owner = Some(pid);
    queue.clear_events();
    queue.held = [0; 4];
    true
}

pub fn release(pid: u64) {
    let mut queue = EVENTS.lock();
    if queue.owner == Some(pid) {
        queue.owner = None;
        queue.clear_events();
        queue.held = [0; 4];
    }
}

pub fn push(usage: u8, pressed: bool, modifiers: u8) -> bool {
    let mut queue = EVENTS.lock();
    if queue.owner.is_none() { return false; }
    let slot = (usage / 64) as usize;
    let mask = 1u64 << (usage % 64);
    if pressed { queue.held[slot] |= mask; } else { queue.held[slot] &= !mask; }

    if queue.len == QUEUE_CAPACITY {
        queue.clear_events();
        let held = queue.held;
        queue.held = [0; 4];
        for (word_index, word) in held.iter().copied().enumerate() {
            for bit in 0..64 {
                if word & (1u64 << bit) != 0 {
                    queue.push(KeyEvent { usage: (word_index * 64 + bit) as u8, pressed: 0, modifiers: 0, _reserved: 0 });
                }
            }
        }
        queue.push(KeyEvent { usage, pressed: pressed as u8, modifiers, _reserved: 0 });
        if pressed { queue.held[slot] |= mask; } else { queue.held[slot] &= !mask; }
        return true;
    }
    queue.push(KeyEvent { usage, pressed: pressed as u8, modifiers, _reserved: 0 });
    true
}

pub fn pop(pid: u64) -> Option<KeyEvent> {
    let mut queue = EVENTS.lock();
    if queue.owner != Some(pid) { return None; }
    queue.pop()
}
