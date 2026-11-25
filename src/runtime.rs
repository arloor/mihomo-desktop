use crate::mihomo::{LogEntry, MemoryInfo, MihomoClient, TrafficPoint};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::runtime::Runtime;

static TOKIO_RT: OnceLock<Runtime> = OnceLock::new();

pub fn tokio_rt() -> &'static Runtime {
    TOKIO_RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(4)
            .thread_name("mihomo-bg")
            .build()
            .expect("failed to build Tokio runtime")
    })
}

pub const MAX_TRAFFIC_HISTORY: usize = 60;
pub const MAX_LOGS: usize = 500;

#[derive(Debug, Default)]
pub struct LiveTelemetry {
    pub current_traffic: TrafficPoint,
    pub traffic_history: VecDeque<TrafficPoint>,
    pub memory: MemoryInfo,
    pub logs: VecDeque<LogEntry>,
    pub logs_paused: bool,
    pub log_level: String,
    pub controller_addr: String,
    pub controller_secret: String,
    pub generation: u64,
}

impl LiveTelemetry {
    pub fn new(controller_addr: String, controller_secret: String) -> Self {
        Self {
            log_level: "info".to_string(),
            controller_addr,
            controller_secret,
            ..Default::default()
        }
    }

    pub fn push_traffic(&mut self, point: TrafficPoint) {
        self.current_traffic = point;
        if self.traffic_history.len() >= MAX_TRAFFIC_HISTORY {
            self.traffic_history.pop_front();
        }
        self.traffic_history.push_back(point);
    }

    pub fn push_log(&mut self, entry: LogEntry) {
        if self.logs_paused {
            return;
        }
        if self.logs.len() >= MAX_LOGS {
            self.logs.pop_front();
        }
        self.logs.push_back(entry);
    }

    pub fn reconfigure(&mut self, addr: String, secret: String, log_level: String) {
        if self.controller_addr != addr
            || self.controller_secret != secret
            || self.log_level != log_level
        {
            self.controller_addr = addr;
            self.controller_secret = secret;
            self.log_level = log_level;
            self.generation = self.generation.wrapping_add(1);
        }
    }
}

/// Spawns persistent background WebSocket listeners for `/traffic`, `/memory`, and `/logs`.
pub fn spawn_telemetry_workers(telemetry: Arc<Mutex<LiveTelemetry>>) {
    let rt = tokio_rt();

    // Traffic WS worker
    {
        let telemetry = Arc::clone(&telemetry);
        rt.spawn(async move {
            loop {
                let (addr, secret, epoch) = {
                    let g = telemetry.lock().unwrap();
                    (
                        g.controller_addr.clone(),
                        g.controller_secret.clone(),
                        g.generation,
                    )
                };
                let client = MihomoClient::new(&addr, &secret);
                let t_ref = Arc::clone(&telemetry);
                let _ = client
                    .stream_traffic(move |pt| {
                        let mut g = t_ref.lock().unwrap();
                        if g.generation != epoch {
                            return false;
                        }
                        g.push_traffic(pt);
                        true
                    })
                    .await;
                // Reset instantaneous speed when disconnected
                {
                    let mut g = telemetry.lock().unwrap();
                    if g.generation == epoch {
                        g.current_traffic = TrafficPoint::default();
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
    }

    // Memory WS worker
    {
        let telemetry = Arc::clone(&telemetry);
        rt.spawn(async move {
            loop {
                let (addr, secret, epoch) = {
                    let g = telemetry.lock().unwrap();
                    (
                        g.controller_addr.clone(),
                        g.controller_secret.clone(),
                        g.generation,
                    )
                };
                let client = MihomoClient::new(&addr, &secret);
                let t_ref = Arc::clone(&telemetry);
                let _ = client
                    .stream_memory(move |mem| {
                        let mut g = t_ref.lock().unwrap();
                        if g.generation != epoch {
                            return false;
                        }
                        g.memory = mem;
                        true
                    })
                    .await;
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        });
    }

    // Logs WS worker
    {
        let telemetry = Arc::clone(&telemetry);
        rt.spawn(async move {
            loop {
                let (addr, secret, level, epoch) = {
                    let g = telemetry.lock().unwrap();
                    (
                        g.controller_addr.clone(),
                        g.controller_secret.clone(),
                        g.log_level.clone(),
                        g.generation,
                    )
                };
                let client = MihomoClient::new(&addr, &secret);
                let t_ref = Arc::clone(&telemetry);
                let _ = client
                    .stream_logs(&level, move |entry| {
                        let mut g = t_ref.lock().unwrap();
                        if g.generation != epoch {
                            return false;
                        }
                        g.push_log(entry);
                        true
                    })
                    .await;
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
    }
}
