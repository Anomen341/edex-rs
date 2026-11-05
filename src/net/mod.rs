//! Network status, traffic and connections, gathered on background threads.
//!
//! Slow or unreliable work (latency, external IP, connection lookups) has its
//! own thread, so a timeout there cannot stall the traffic graph, and the UI
//! only ever reads finished snapshots. Everything that needs the internet is
//! optional and reports "not available" by simply staying empty.

mod connections;
mod probe;
mod traffic;

use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::mpsc::{sync_channel, Receiver, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::Config;

/// How often traffic is sampled.
pub const TRAFFIC_INTERVAL: Duration = Duration::from_millis(500);
/// Samples kept for the traffic graph: one minute.
pub const TRAFFIC_HISTORY: usize = 120;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Link {
    /// Nothing measured yet.
    #[default]
    Unknown,
    /// No network route at all.
    Offline,
    /// On a network, but the latency host cannot be reached.
    LocalOnly,
    Online,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoPoint {
    pub latitude: f32,
    pub longitude: f32,
}

#[derive(Clone, Debug, Default)]
pub struct NetSnapshot {
    pub link: Link,
    pub interface: Option<String>,
    pub ipv4: Option<Ipv4Addr>,
    pub external_ip: Option<IpAddr>,
    pub latency_ms: Option<u32>,
    /// Bytes per second, oldest first.
    pub up_history: Vec<f32>,
    pub down_history: Vec<f32>,
    pub up_rate: Option<f32>,
    pub down_rate: Option<f32>,
    pub total_up: Option<u64>,
    pub total_down: Option<u64>,
    /// Established connections to other machines.
    pub connections: Option<usize>,
    /// Located remote hosts. Empty without a GeoIP database.
    pub points: Vec<GeoPoint>,
    pub geoip_loaded: bool,
    pub sampled_at: Option<Instant>,
}

/// What the slow threads have found, for the traffic thread to send on.
#[derive(Clone, Debug, Default)]
struct Findings {
    link: Link,
    ipv4: Option<Ipv4Addr>,
    external_ip: Option<IpAddr>,
    latency_ms: Option<u32>,
    connections: Option<usize>,
    points: Vec<GeoPoint>,
    geoip_loaded: bool,
}

type Shared = Arc<Mutex<Findings>>;

/// A panicked thread must not take the others down with it.
fn lock(shared: &Shared) -> MutexGuard<'_, Findings> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Start everything. If a thread cannot start, its panel just stays empty.
pub fn spawn(config: &Config) -> Receiver<NetSnapshot> {
    let shared = Shared::default();
    let (sender, receiver) = sync_channel(1);

    let probe_shared = shared.clone();
    let targets = probe::Targets {
        latency_host: config.latency_host.clone(),
        external_ip_url: config.external_ip_url.clone(),
    };
    let _ = thread::Builder::new()
        .name("net-probe".to_owned())
        .spawn(move || probe::run(&targets, &probe_shared));

    let connection_shared = shared.clone();
    let _ = thread::Builder::new()
        .name("net-connections".to_owned())
        .spawn(move || connections::run(&connection_shared));

    let _ = thread::Builder::new().name("net-traffic".to_owned()).spawn(move || {
        let mut meter = traffic::Meter::new();
        loop {
            let started = Instant::now();
            let findings = lock(&shared).clone();
            let snapshot = meter.sample(findings);
            if let Err(TrySendError::Disconnected(_)) = sender.try_send(snapshot) {
                return;
            }
            thread::sleep(TRAFFIC_INTERVAL.saturating_sub(started.elapsed()));
        }
    });
    receiver
}

/// Add a sample to a fixed-length history.
fn push_sample(history: &mut VecDeque<f32>, value: f32) {
    if history.len() == TRAFFIC_HISTORY {
        history.pop_front();
    }
    history.push_back(value);
}
