const SERVICE_CONTROL_STOP: u32 = 1;
const SERVICE_CONTROL_SHUTDOWN: u32 = 5;
const SERVICE_CONTROL_SESSIONCHANGE: u32 = 14;

pub const ACCEPTED_CONTROL_MASK: u32 = 0x0000_0001 | 0x0000_0004 | 0x0000_0080;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceControl {
    Stop,
    Shutdown,
    SessionChange { event_type: u32, session_id: u32 },
}

impl ServiceControl {
    pub const fn from_raw(control: u32, event_type: u32) -> Option<Self> {
        match control {
            SERVICE_CONTROL_STOP => Some(Self::Stop),
            SERVICE_CONTROL_SHUTDOWN => Some(Self::Shutdown),
            SERVICE_CONTROL_SESSIONCHANGE => Some(Self::from_session_change(event_type, 0)),
            _ => None,
        }
    }

    pub const fn from_session_change(event_type: u32, session_id: u32) -> Self {
        Self::SessionChange {
            event_type,
            session_id,
        }
    }
}
