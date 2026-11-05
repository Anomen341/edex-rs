//! Open connections, placed on the globe if a local GeoIP database exists.

use std::collections::BTreeSet;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use maxminddb::{geoip2, Reader};
use netstat2::{get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState};

use super::{lock, GeoPoint, Shared};
use crate::config::config_dir;

const SCAN_EVERY: Duration = Duration::from_secs(3);
/// The file name the README tells people to use.
const DATABASE_NAME: &str = "GeoLite2-City.mmdb";
/// More points than this only clutters the globe.
const MAX_POINTS: usize = 64;

pub fn run(shared: &Shared) {
    let database = database_path().and_then(|path| open_database(&path));
    lock(shared).geoip_loaded = database.is_some();
    loop {
        let remotes = remote_addresses();
        let count = remotes.as_ref().map(Vec::len);
        let points = match (&database, &remotes) {
            (Some(database), Some(remotes)) => locate(database, remotes),
            _ => Vec::new(),
        };
        {
            let mut findings = lock(shared);
            findings.connections = count;
            findings.points = points;
        }
        thread::sleep(SCAN_EVERY);
    }
}

fn database_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join(DATABASE_NAME))
}

/// A missing or damaged file just means no points on the globe.
fn open_database(path: &Path) -> Option<Reader<Vec<u8>>> {
    Reader::open_readfile(path).ok()
}

fn remote_addresses() -> Option<Vec<IpAddr>> {
    let families = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let sockets = get_sockets_info(families, ProtocolFlags::TCP).ok()?;
    let remotes = sockets.into_iter().filter_map(|socket| match socket.protocol_socket_info {
        ProtocolSocketInfo::Tcp(tcp) if tcp.state == TcpState::Established => Some(tcp.remote_addr),
        _ => None,
    });
    Some(remotes.filter(|ip| !ip.is_loopback() && !ip.is_unspecified()).collect())
}

/// Addresses that could be somewhere else in the world.
fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => !(ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified()),
        IpAddr::V6(ip) => {
            let first = ip.segments()[0];
            let local = first & 0xFE00 == 0xFC00 || first & 0xFFC0 == 0xFE80;
            !(local || ip.is_loopback() || ip.is_unspecified())
        }
    }
}

fn locate(database: &Reader<Vec<u8>>, remotes: &[IpAddr]) -> Vec<GeoPoint> {
    let unique: BTreeSet<IpAddr> = remotes.iter().copied().filter(|ip| is_public(*ip)).collect();
    unique.into_iter().filter_map(|ip| locate_one(database, ip)).take(MAX_POINTS).collect()
}

fn locate_one(database: &Reader<Vec<u8>>, ip: IpAddr) -> Option<GeoPoint> {
    let result = database.lookup(ip).ok()?;
    let city: geoip2::City<'_> = result.decode().ok()??;
    Some(GeoPoint {
        latitude: city.location.latitude? as f32,
        longitude: city.location.longitude? as f32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_database() -> Reader<Vec<u8>> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/test/GeoIP2-City-Test.mmdb");
        open_database(&path).expect("test database")
    }

    #[test]
    fn known_addresses_land_in_the_right_place() {
        let database = test_database();
        let london = locate_one(&database, "81.2.69.160".parse().unwrap()).unwrap();
        assert!((london.latitude - 51.5).abs() < 1.0 && (london.longitude + 0.1).abs() < 1.0, "{london:?}");
    }

    #[test]
    fn unknown_private_and_repeated_addresses_are_skipped() {
        let database = test_database();
        let remotes: Vec<IpAddr> = ["192.168.1.1", "8.8.8.8", "81.2.69.160", "81.2.69.160"]
            .iter()
            .map(|ip| ip.parse().unwrap())
            .collect();
        assert_eq!(locate(&database, &remotes).len(), 1);
    }

    #[test]
    fn a_missing_or_damaged_database_is_not_an_error() {
        assert!(open_database(Path::new("/nonexistent/file.mmdb")).is_none());
        let damaged = std::env::temp_dir().join(format!("edex-bad-{}.mmdb", std::process::id()));
        std::fs::write(&damaged, b"not a database").unwrap();
        assert!(open_database(&damaged).is_none());
        std::fs::remove_file(damaged).unwrap();
    }

    #[test]
    fn only_public_addresses_count() {
        for ip in ["10.1.2.3", "192.168.0.5", "172.16.0.1", "169.254.1.1", "127.0.0.1", "::1", "fe80::1", "fd00::1"] {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["8.8.8.8", "2606:4700::1111"] {
            assert!(is_public(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn this_machines_connections_can_be_read() {
        assert!(remote_addresses().is_some());
    }
}
