use mactype_service_host::{ProcessArchitecture, ProcessIdentity};

pub(crate) fn identity(pid: u32, creation_time: u64) -> ProcessIdentity {
    ProcessIdentity {
        pid,
        creation_time,
        session_id: 2,
        architecture: ProcessArchitecture::X64,
        protected: false,
    }
}
