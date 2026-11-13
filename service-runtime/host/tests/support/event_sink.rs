#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mactype_service_host::{HostEvent, HostEventLogger, HostEventSink};

pub(crate) fn discard_events() -> Arc<dyn HostEventSink> {
    Arc::new(DiscardEventSink)
}

struct DiscardEventSink;

impl HostEventSink for DiscardEventSink {
    fn record(&self, _event: HostEvent) {}
}

pub(crate) struct RecordingEventSink {
    events: Mutex<Vec<HostEvent>>,
    logger: HostEventLogger,
    now: Mutex<Instant>,
    path: PathBuf,
}

impl RecordingEventSink {
    pub(crate) fn new(path: PathBuf, now: Instant) -> Arc<Self> {
        Arc::new(Self {
            events: Mutex::new(Vec::new()),
            logger: HostEventLogger::new_at(path.clone(), now),
            now: Mutex::new(now),
            path,
        })
    }

    pub(crate) fn advance(&self, duration: Duration) {
        let mut now = self.now.lock().unwrap();
        *now += duration;
    }

    pub(crate) fn events(&self) -> Vec<HostEvent> {
        self.events.lock().unwrap().clone()
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl HostEventSink for RecordingEventSink {
    fn record(&self, event: HostEvent) {
        self.events.lock().unwrap().push(event.clone());
        self.logger.record_at(event, *self.now.lock().unwrap());
    }
}
