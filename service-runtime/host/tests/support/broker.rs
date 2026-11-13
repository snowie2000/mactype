#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::Mutex;

use mactype_service_host::{BrokerResult, InjectionBroker, InjectionRequest};

pub(crate) struct ScriptedBroker {
    results: Mutex<VecDeque<BrokerResult>>,
    requests: Mutex<Vec<InjectionRequest>>,
}

impl ScriptedBroker {
    pub(crate) fn new(results: impl IntoIterator<Item = BrokerResult>) -> Self {
        Self {
            results: Mutex::new(results.into_iter().collect()),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }

    pub(crate) fn requests(&self) -> Vec<InjectionRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl InjectionBroker for ScriptedBroker {
    fn inject(&self, request: &InjectionRequest) -> BrokerResult {
        self.requests.lock().unwrap().push(request.clone());
        self.results
            .lock()
            .unwrap()
            .pop_front()
            .expect("the scripted broker result queue is exhausted")
    }
}
