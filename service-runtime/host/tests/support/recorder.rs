#![allow(dead_code)]

use std::io;
use std::sync::Mutex;

use mactype_service_contract::HealthReport;
use mactype_service_host::{HealthPublisher, ServiceStatus, StatusReporter};

#[derive(Clone, Default)]
pub(crate) struct RecorderState {
    pub(crate) events: Vec<String>,
    pub(crate) reports: Vec<HealthReport>,
    pub(crate) statuses: Vec<ServiceStatus>,
}

#[derive(Default)]
pub(crate) struct Recorder {
    pub(crate) state: Mutex<RecorderState>,
}

impl Recorder {
    pub(crate) fn reports(&self) -> Vec<HealthReport> {
        self.state.lock().unwrap().reports.clone()
    }

    fn record(&self, event: String, report: Option<&HealthReport>, status: Option<ServiceStatus>) {
        let mut state = self.state.lock().unwrap();
        state.events.push(event);
        if let Some(report) = report {
            state.reports.push(report.clone());
        }
        if let Some(status) = status {
            state.statuses.push(status);
        }
    }
}

impl StatusReporter for Recorder {
    fn report(&self, status: ServiceStatus) -> io::Result<()> {
        self.record(format!("scm:{:?}", status.state), None, Some(status));
        Ok(())
    }
}

impl HealthPublisher for Recorder {
    fn publish(&self, report: &HealthReport) -> io::Result<()> {
        self.record(format!("health:{:?}", report.health), Some(report), None);
        Ok(())
    }
}
