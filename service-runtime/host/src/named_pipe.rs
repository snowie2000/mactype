use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use mactype_service_contract::HealthReport;
use mactype_service_platform::{ConnectOutcome, NamedPipeServer, SecurityDescriptor};

use crate::HealthPublisher;

pub const HEALTH_PIPE_SECURITY_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GR;;;AU)";
const MAX_HEALTH_MESSAGE_BYTES: usize = 16 * 1024;

pub struct NamedPipeHealthPublisher {
    current: Arc<Mutex<Vec<u8>>>,
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl NamedPipeHealthPublisher {
    pub fn start(pipe_name: &str) -> io::Result<Self> {
        let descriptor = SecurityDescriptor::from_sddl(HEALTH_PIPE_SECURITY_SDDL)?;
        let server = NamedPipeServer::create(pipe_name, &descriptor, 16 * 1024)?;

        let current = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_current = Arc::clone(&current);
        let worker_stop = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            serve(server, worker_current, worker_stop);
        });

        Ok(Self {
            current,
            stop,
            worker: Mutex::new(Some(worker)),
        })
    }
}

impl HealthPublisher for NamedPipeHealthPublisher {
    fn publish(&self, report: &HealthReport) -> io::Result<()> {
        report
            .validate()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let mut bytes = serde_json::to_vec(report)?;
        bytes.push(b'\n');
        if bytes.len() > MAX_HEALTH_MESSAGE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "health report exceeds the fixed named-pipe message bound",
            ));
        }
        *self
            .current
            .lock()
            .map_err(|_| io::Error::other("health report lock poisoned"))? = bytes;
        Ok(())
    }
}

impl Drop for NamedPipeHealthPublisher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.lock().ok().and_then(|mut worker| worker.take()) {
            let _ = worker.join();
        }
    }
}

fn serve(server: NamedPipeServer, current: Arc<Mutex<Vec<u8>>>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Acquire) {
        match server.connect() {
            Ok(ConnectOutcome::Connected) => {
                let bytes = match current.lock() {
                    Ok(current) => current.clone(),
                    Err(poisoned) => poisoned.into_inner().clone(),
                };
                if !bytes.is_empty() {
                    let _ = server.write_message(&bytes);
                }
                server.disconnect();
            }
            Ok(ConnectOutcome::Listening) => {}
            Err(_) => server.disconnect(),
        }
        thread::sleep(Duration::from_millis(25));
    }
    // Dropping the server disconnects any attached client and closes the pipe.
}
