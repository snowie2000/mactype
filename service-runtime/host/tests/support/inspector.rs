#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use mactype_service_contract::StructuredServiceError;
use mactype_service_host::{
    InspectedProcess, ProcessIdentity, ProcessInspector, TargetLifecycle, TargetLiveness,
};

#[derive(Clone)]
pub(crate) struct InspectorResponse {
    inspected: Result<InspectedProcess, StructuredServiceError>,
    lifecycle: TargetLifecycle,
    liveness: TargetLiveness,
    basename: Option<String>,
}

impl InspectorResponse {
    pub(crate) fn inspected(inspected: InspectedProcess, lifecycle: TargetLifecycle) -> Self {
        let basename = inspected.facts.image_name.clone();
        Self {
            inspected: Ok(inspected),
            lifecycle,
            liveness: TargetLiveness::Unknown,
            basename,
        }
    }

    pub(crate) fn failure(error: StructuredServiceError) -> Self {
        Self {
            inspected: Err(error),
            lifecycle: TargetLifecycle::Unknown,
            liveness: TargetLiveness::Unknown,
            basename: None,
        }
    }

    pub(crate) fn with_liveness(mut self, liveness: TargetLiveness) -> Self {
        self.liveness = liveness;
        self
    }

    pub(crate) fn with_basename(mut self, basename: impl Into<String>) -> Self {
        self.basename = Some(basename.into());
        self
    }
}

pub(crate) struct ScriptedInspector {
    responses: Mutex<HashMap<u32, VecDeque<InspectorResponse>>>,
    active: Mutex<HashMap<(u32, u64), InspectorResponse>>,
    liveness_probes: Mutex<Vec<ProcessIdentity>>,
}

impl ScriptedInspector {
    pub(crate) fn new(responses: impl IntoIterator<Item = (u32, InspectorResponse)>) -> Self {
        let mut scripts = HashMap::<u32, VecDeque<InspectorResponse>>::new();
        for (pid, response) in responses {
            scripts.entry(pid).or_default().push_back(response);
        }
        Self {
            responses: Mutex::new(scripts),
            active: Mutex::new(HashMap::new()),
            liveness_probes: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn set_lifecycle(&self, identity: &ProcessIdentity, lifecycle: TargetLifecycle) {
        self.active
            .lock()
            .unwrap()
            .get_mut(&(identity.pid, identity.creation_time))
            .expect("the scripted identity must be inspected before its lifecycle changes")
            .lifecycle = lifecycle;
    }

    pub(crate) fn liveness_probes(&self) -> Vec<ProcessIdentity> {
        self.liveness_probes.lock().unwrap().clone()
    }
}

impl ProcessInspector for ScriptedInspector {
    fn inspect(&self, pid: u32) -> Result<InspectedProcess, StructuredServiceError> {
        let response = self
            .responses
            .lock()
            .unwrap()
            .get_mut(&pid)
            .and_then(VecDeque::pop_front)
            .unwrap_or_else(|| panic!("no scripted inspection response remains for PID {pid}"));
        if let Ok(inspected) = &response.inspected {
            self.active.lock().unwrap().insert(
                (inspected.identity.pid, inspected.identity.creation_time),
                response.clone(),
            );
        }
        response.inspected
    }

    fn probe_target_lifecycle(&self, identity: &ProcessIdentity) -> TargetLifecycle {
        self.active
            .lock()
            .unwrap()
            .get(&(identity.pid, identity.creation_time))
            .map_or(TargetLifecycle::Unknown, |response| response.lifecycle)
    }

    fn process_basename(&self, identity: &ProcessIdentity) -> Option<String> {
        self.active
            .lock()
            .unwrap()
            .get(&(identity.pid, identity.creation_time))
            .and_then(|response| response.basename.clone())
    }

    fn probe_target_liveness(&self, identity: &ProcessIdentity) -> TargetLiveness {
        self.liveness_probes.lock().unwrap().push(identity.clone());
        self.active
            .lock()
            .unwrap()
            .get(&(identity.pid, identity.creation_time))
            .map_or(TargetLiveness::Unknown, |response| response.liveness)
    }
}
