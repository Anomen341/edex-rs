//! Upload and download rates for the active interface.

use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Instant;

use sysinfo::Networks;

use super::{push_sample, Findings, NetSnapshot};

/// One interface's counters since the last refresh.
#[derive(Clone, Debug, PartialEq)]
struct Reading {
    name: String,
    addresses: Vec<IpAddr>,
    sent: u64,
    received: u64,
    total_sent: u64,
    total_received: u64,
}

pub struct Meter {
    networks: Option<Networks>,
    up: VecDeque<f32>,
    down: VecDeque<f32>,
    last: Instant,
}

impl Meter {
    pub fn new() -> Self {
        Self {
            networks: Networks::new_with_refreshed_list().ok(),
            up: VecDeque::new(),
            down: VecDeque::new(),
            last: Instant::now(),
        }
    }

    pub fn sample(&mut self, findings: Findings) -> NetSnapshot {
        let now = Instant::now();
        let seconds = now.duration_since(self.last).as_secs_f32().max(0.001);
        self.last = now;

        let mut snapshot = NetSnapshot {
            link: findings.link,
            ipv4: findings.ipv4,
            external_ip: findings.external_ip,
            latency_ms: findings.latency_ms,
            connections: findings.connections,
            points: findings.points,
            geoip_loaded: findings.geoip_loaded,
            sampled_at: Some(now),
            ..NetSnapshot::default()
        };
        let Some(networks) = &mut self.networks else { return snapshot };
        networks.refresh(true);
        let readings: Vec<Reading> = networks
            .iter()
            .map(|(name, data)| Reading {
                name: name.to_string_lossy().into_owned(),
                addresses: data.ip_networks().iter().map(|network| network.addr).collect(),
                sent: data.transmitted(),
                received: data.received(),
                total_sent: data.total_transmitted(),
                total_received: data.total_received(),
            })
            .collect();

        if let Some(active) = pick_interface(&readings, findings.ipv4) {
            let (up, down) = (active.sent as f32 / seconds, active.received as f32 / seconds);
            push_sample(&mut self.up, up);
            push_sample(&mut self.down, down);
            snapshot.interface = Some(active.name.clone());
            snapshot.up_rate = Some(up);
            snapshot.down_rate = Some(down);
            snapshot.total_up = Some(active.total_sent);
            snapshot.total_down = Some(active.total_received);
        }
        snapshot.up_history = self.up.iter().copied().collect();
        snapshot.down_history = self.down.iter().copied().collect();
        snapshot
    }
}

/// The interface holding the address used to reach the internet, or failing
/// that the busiest one that isn't loopback.
fn pick_interface(readings: &[Reading], ipv4: Option<Ipv4Addr>) -> Option<&Reading> {
    let wanted = ipv4.map(IpAddr::V4);
    let by_address = wanted.and_then(|ip| readings.iter().find(|r| r.addresses.contains(&ip)));
    by_address.or_else(|| {
        readings
            .iter()
            .filter(|r| !r.addresses.iter().any(IpAddr::is_loopback) && r.name != "lo")
            .max_by_key(|r| r.total_sent.saturating_add(r.total_received))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(name: &str, address: &str, total: u64) -> Reading {
        Reading {
            name: name.to_owned(),
            addresses: vec![address.parse().unwrap()],
            sent: 0,
            received: 0,
            total_sent: total,
            total_received: 0,
        }
    }

    #[test]
    fn the_interface_with_the_route_address_wins() {
        let list = [reading("eth0", "10.0.0.5", 5), reading("wifi", "192.168.1.9", 900)];
        let picked = pick_interface(&list, Some("10.0.0.5".parse().unwrap()));
        assert_eq!(picked.unwrap().name, "eth0");
    }

    #[test]
    fn without_a_route_the_busiest_real_interface_is_used() {
        let list = [reading("lo", "127.0.0.1", 9_000), reading("eth0", "10.0.0.5", 5)];
        assert_eq!(pick_interface(&list, None).unwrap().name, "eth0");
    }

    #[test]
    fn having_no_interfaces_is_fine() {
        assert!(pick_interface(&[], None).is_none());
    }

    #[test]
    fn history_is_capped_and_rates_are_produced() {
        let mut meter = Meter::new();
        for _ in 0..3 {
            let snapshot = meter.sample(Findings::default());
            assert!(snapshot.sampled_at.is_some());
        }
        let mut history = VecDeque::new();
        for i in 0..super::super::TRAFFIC_HISTORY + 5 {
            push_sample(&mut history, i as f32);
        }
        assert_eq!(history.len(), super::super::TRAFFIC_HISTORY);
        assert_eq!(history[0], 5.0);
    }
}
