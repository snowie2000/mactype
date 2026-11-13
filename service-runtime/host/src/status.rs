use std::io;

pub const SERVICE_STOP_WAIT_HINT_MS: u32 = 25_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScmState {
    StartPending,
    Running,
    StopPending,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceStatus {
    pub state: ScmState,
    pub checkpoint: u32,
    pub wait_hint_ms: u32,
    pub win32_exit_code: u32,
    pub service_specific_exit_code: u32,
}

impl ServiceStatus {
    pub const fn start_pending(checkpoint: u32, wait_hint_ms: u32) -> Self {
        Self::pending(ScmState::StartPending, checkpoint, wait_hint_ms)
    }

    pub const fn running() -> Self {
        Self::settled(ScmState::Running, 0, 0)
    }

    pub const fn stop_pending(checkpoint: u32, wait_hint_ms: u32) -> Self {
        Self::pending(ScmState::StopPending, checkpoint, wait_hint_ms)
    }

    pub const fn stopped() -> Self {
        Self::settled(ScmState::Stopped, 0, 0)
    }

    pub const fn stopped_with_error(win32_error: u32, service_error: u32) -> Self {
        Self::settled(ScmState::Stopped, win32_error, service_error)
    }

    const fn pending(state: ScmState, checkpoint: u32, wait_hint_ms: u32) -> Self {
        Self {
            state,
            checkpoint,
            wait_hint_ms,
            win32_exit_code: 0,
            service_specific_exit_code: 0,
        }
    }

    const fn settled(
        state: ScmState,
        win32_exit_code: u32,
        service_specific_exit_code: u32,
    ) -> Self {
        Self {
            state,
            checkpoint: 0,
            wait_hint_ms: 0,
            win32_exit_code,
            service_specific_exit_code,
        }
    }
}

pub trait StatusReporter: Send + Sync {
    fn report(&self, status: ServiceStatus) -> io::Result<()>;
}
