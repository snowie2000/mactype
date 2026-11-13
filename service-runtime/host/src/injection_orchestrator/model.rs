use crate::target_validation::DeferralReason;
use std::time::{Duration, Instant};

use crate::ProcessIdentity;

pub const MAX_DEFERRED_TARGETS: usize = 512;

pub const MAX_TRACKED_PROCESS_RESULTS: usize = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u8,
    pub initial_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(25),
            max_delay: Duration::from_millis(250),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeferralPolicy {
    /// How long a frozen target waits before its lifecycle is probed again.
    pub frozen_recheck: Duration,
    /// First wait after a pre-resume launch failure; doubles per deferral.
    pub launch_initial_delay: Duration,
    pub launch_max_delay: Duration,
    /// Deferrals allowed for one identity after launch failures before it is rejected.
    pub launch_max_deferrals: u8,
}

impl Default for DeferralPolicy {
    fn default() -> Self {
        Self {
            frozen_recheck: Duration::from_secs(2),
            launch_initial_delay: Duration::from_secs(2),
            launch_max_delay: Duration::from_secs(32),
            launch_max_deferrals: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeferredTarget {
    pub identity: ProcessIdentity,
    pub reason: DeferralReason,
    pub deferrals: u8,
    pub not_before: Instant,
}

pub trait RetryScheduler {
    fn wait(&self, delay: Duration) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessOutcome {
    Injected,
    Skipped,
    Deferred,
    Duplicate,
    Rejected,
    RetryExhausted,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessAttemptRecord {
    pub identity: ProcessIdentity,
    pub runtime_generation_id: String,
    pub outcome: ProcessOutcome,
    pub attempts: u8,
    pub code: String,
    pub win32_error: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionChange {
    pub event_type: u32,
    pub session_id: u32,
}

impl SessionChange {
    const OVERFLOW_EVENT_TYPE: u32 = u32::MAX;

    pub const fn overflow() -> Self {
        Self {
            event_type: Self::OVERFLOW_EVENT_TYPE,
            session_id: 0,
        }
    }

    pub const fn is_overflow(self) -> bool {
        self.event_type == Self::OVERFLOW_EVENT_TYPE
    }
}
