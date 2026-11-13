use mactype_service_contract::{MachinePaths, StructuredServiceError};
use std::sync::Arc;

use crate::{
    EtwProcessEventSource, FixedHelperBroker, HostEvent, HostEventSink, InitializedRuntime,
    LiveObserver, PROCESS_START_SESSION, ProcessEventSource, ProtectedProfileInitializer,
    ProtectedRuntimeAssets, RuntimeInitializer, WindowsHelperLauncher, WindowsProcessInspector,
    WindowsStartupSafety, WmiProcessEventSource, initialize_process_orchestration,
};

pub struct WindowsOpenServiceInitializer {
    paths: MachinePaths,
    events: Arc<dyn HostEventSink>,
}

impl WindowsOpenServiceInitializer {
    pub fn new(paths: MachinePaths, events: Arc<dyn HostEventSink>) -> Self {
        Self { paths, events }
    }
}

impl RuntimeInitializer for WindowsOpenServiceInitializer {
    fn initialize(&self) -> Result<InitializedRuntime, StructuredServiceError> {
        let profile_initializer = ProtectedProfileInitializer::new(self.paths.clone());
        let prepared_profile = profile_initializer.prepare()?;
        let generation = profile_initializer.resolve_generation(&prepared_profile)?;
        let profile =
            profile_initializer.initialize_with_generation(prepared_profile, &generation)?;
        let assets = ProtectedRuntimeAssets::load_from_generation(&generation)?;
        WindowsStartupSafety::verify(&assets.root().join("mactype-service.exe"))?;
        let source = self.live_process_event_source()?;
        let service_pid = std::process::id();
        let inspector = WindowsProcessInspector::new();
        let launcher = WindowsHelperLauncher::new(crate::scm::stop_requested);
        let broker = FixedHelperBroker::new(&assets, launcher, self.events.clone());
        initialize_process_orchestration(
            profile.active_profile_digest,
            service_pid,
            assets.generation_id(),
            source,
            Box::new(inspector),
            Box::new(broker),
            self.events.clone(),
        )
    }
}

impl WindowsOpenServiceInitializer {
    /// The fastest process event source this machine will give us.
    ///
    /// A private real-time ETW session hears about a new process in tens of
    /// milliseconds, where WMI's shared session takes hundreds. Opening one
    /// needs an elevated or LocalSystem token, which the service has and a
    /// developer shell may not, so a refusal falls back to WMI rather than
    /// failing the start. The two behave identically from here on; only the
    /// summary detail says which one is live.
    fn live_process_event_source(
        &self,
    ) -> Result<Box<dyn ProcessEventSource>, StructuredServiceError> {
        let (source, observer): (Box<dyn ProcessEventSource>, LiveObserver) =
            match EtwProcessEventSource::start(PROCESS_START_SESSION) {
                Ok(source) => (Box::new(source), LiveObserver::Etw),
                Err(_) => (
                    Box::new(WmiProcessEventSource::connect()?),
                    LiveObserver::Wmi,
                ),
            };
        self.events
            .record(HostEvent::LiveObserverSelected(observer));
        Ok(source)
    }
}
