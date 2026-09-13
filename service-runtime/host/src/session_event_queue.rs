use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use crate::SessionChange;

pub(crate) const SESSION_EVENT_QUEUE_CAPACITY: usize = 64;

pub(crate) struct SessionEventQueue {
    slots: [AtomicU64; SESSION_EVENT_QUEUE_CAPACITY],
    write_cursor: AtomicUsize,
    read_cursor: AtomicUsize,
    overflowed: AtomicBool,
}

impl SessionEventQueue {
    pub(crate) const fn new() -> Self {
        Self {
            slots: [const { AtomicU64::new(0) }; SESSION_EVENT_QUEUE_CAPACITY],
            write_cursor: AtomicUsize::new(0),
            read_cursor: AtomicUsize::new(0),
            overflowed: AtomicBool::new(false),
        }
    }

    pub(crate) fn push(&self, event_type: u32, session_id: u32) -> bool {
        let raw = (u64::from(event_type) << 32) | u64::from(session_id);
        let Some(encoded) = raw.checked_add(1) else {
            self.overflowed.store(true, Ordering::Release);
            return false;
        };
        let start = self.write_cursor.fetch_add(1, Ordering::Relaxed);
        for offset in 0..SESSION_EVENT_QUEUE_CAPACITY {
            let index = start.wrapping_add(offset) % SESSION_EVENT_QUEUE_CAPACITY;
            if self.slots[index]
                .compare_exchange(0, encoded, Ordering::Release, Ordering::Relaxed)
                .is_ok()
            {
                return true;
            }
        }
        self.overflowed.store(true, Ordering::Release);
        false
    }

    pub(crate) fn pop(&self) -> Option<SessionChange> {
        if self.overflowed.swap(false, Ordering::AcqRel) {
            return Some(SessionChange::overflow());
        }
        let start = self.read_cursor.load(Ordering::Relaxed);
        for offset in 0..SESSION_EVENT_QUEUE_CAPACITY {
            let index = start.wrapping_add(offset) % SESSION_EVENT_QUEUE_CAPACITY;
            let encoded = self.slots[index].swap(0, Ordering::AcqRel);
            if encoded != 0 {
                self.read_cursor
                    .store(index.wrapping_add(1), Ordering::Relaxed);
                let raw = encoded - 1;
                return Some(SessionChange {
                    event_type: (raw >> 32) as u32,
                    session_id: raw as u32,
                });
            }
        }
        None
    }
}
