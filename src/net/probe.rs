//! Route, latency and external IP. All plain sockets: TCP connect for latency
//! so no admin rights are needed, and one small HTTP request for the external IP.

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

use super::{lock, Link, Shared};

const PROBE_EVERY: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// Rarely changes, and a public service shouldn't be asked often.
const EXTERNAL_IP_EVERY: Duration = Duration::from_secs(600);
const EXTERNAL_IP_RETRY: Duration = Duration::from_secs(60);
const RESPONSE_LIMIT: u64 = 4096;

pub struct Targets {
    pub latency_host: String,
    /// Empty turns the external IP lookup off.
    pub external_ip_url: String,
}

pub fn run(targets: &Targets, shared: &Shared) {
    let mut next_external = Instant::now();
    loop {
        let ipv4 = route_address();
        let latency = ipv4.and_then(|_| tcp_latency(&targets.latency_host, CONNECT_TIMEOUT));
        {
            let mut findings = lock(shared);
            findings.ipv4 = ipv4;
            findings.latency_ms = latency;
            findings.link = link_state(ipv4.is_some(), latency.is_some());
            if ipv4.is_none() {
                findings.external_ip = None;
            }
        }
        if latency.is_some() && Instant::now() >= next_external && !targets.external_ip_url.is_empty() {
            let found = external_ip(&targets.external_ip_url, CONNECT_TIMEOUT);
            next_external = Instant::now() + if found.is_some() { EXTERNAL_IP_EVERY } else { EXTERNAL_IP_RETRY };
            if found.is_some() {
                lock(shared).external_ip = found;
            }
        }
        thread::sleep(PROBE_EVERY);
    }
}

fn link_state(has_route: bool, reached_host: bool) -> Link {
    match (has_route, reached_host) {
        (false, _) => Link::Offline,
        (true, false) => Link::LocalOnly,
        (true, true) => Link::Online,
    }
}

/// The local address the system would use for the internet. Connecting a UDP
/// socket only picks a route: no packet is sent.
fn route_address() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("1.1.1.1:53").ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

fn resolve(host: &str) -> Option<SocketAddr> {
    host.to_socket_addrs().ok()?.next()
}

/// Time for a TCP connect to `host:port`.
fn tcp_latency(host: &str, timeout: Duration) -> Option<u32> {
    let address = resolve(host)?;
    let start = Instant::now();
    TcpStream::connect_timeout(&address, timeout).ok()?;
    Some(start.elapsed().as_millis().max(1) as u32)
}

/// Split `http://host[:port]/path`. HTTPS would need a TLS library for a value
/// that is only displayed, so it isn't supported.
fn parse_url(url: &str) -> Option<(String, String, String)> {
    let rest = url.strip_prefix("http://")?;
    let (authority, path) = rest.split_once('/').map_or((rest, ""), |(a, p)| (a, p));
    if authority.is_empty() {
        return None;
    }
    let address = if authority.contains(':') { authority.to_owned() } else { format!("{authority}:80") };
    let host = authority.split(':').next()?.to_owned();
    Some((address, host, format!("/{path}")))
}

fn external_ip(url: &str, timeout: Duration) -> Option<IpAddr> {
    let (address, host, path) = parse_url(url)?;
    let mut stream = TcpStream::connect_timeout(&resolve(&address)?, timeout).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;
    let request = format!("GET {path} HTTP/1.0\r\nHost: {host}\r\nUser-Agent: edex-rs\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).ok()?;
    let mut response = String::new();
    stream.take(RESPONSE_LIMIT).read_to_string(&mut response).ok()?;
    parse_ip_response(&response)
}

/// Accept only a 200 response whose whole body is one IP address.
fn parse_ip_response(response: &str) -> Option<IpAddr> {
    let (head, body) = response.split_once("\r\n\r\n")?;
    let status = head.lines().next()?;
    if status.split_whitespace().nth(1) != Some("200") {
        return None;
    }
    body.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn link_states_cover_each_case() {
        assert_eq!(link_state(false, false), Link::Offline);
        assert_eq!(link_state(true, false), Link::LocalOnly);
        assert_eq!(link_state(true, true), Link::Online);
    }

    #[test]
    fn latency_measures_a_real_connect() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let host = listener.local_addr().unwrap().to_string();
        assert!(tcp_latency(&host, CONNECT_TIMEOUT).is_some());
    }

    #[test]
    fn a_closed_port_or_bad_host_gives_nothing() {
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        assert_eq!(tcp_latency(&format!("127.0.0.1:{port}"), CONNECT_TIMEOUT), None);
        assert_eq!(tcp_latency("not a host", CONNECT_TIMEOUT), None);
        assert_eq!(tcp_latency("", CONNECT_TIMEOUT), None);
    }

    #[test]
    fn urls_are_split_or_refused() {
        let parsed = parse_url("http://example.org/ip").unwrap();
        assert_eq!(parsed, ("example.org:80".to_owned(), "example.org".to_owned(), "/ip".to_owned()));
        assert_eq!(parse_url("http://127.0.0.1:8080").unwrap().0, "127.0.0.1:8080");
        assert_eq!(parse_url("https://example.org/"), None);
        assert_eq!(parse_url("http://"), None);
        assert_eq!(parse_url(""), None);
    }

    #[test]
    fn only_a_clean_success_counts_as_an_address() {
        let ok = "HTTP/1.1 200 OK\r\nServer: x\r\n\r\n203.0.113.7\n";
        assert_eq!(parse_ip_response(ok), Some("203.0.113.7".parse().unwrap()));
        let v6 = "HTTP/1.1 200 OK\r\n\r\n2001:db8::1";
        assert_eq!(parse_ip_response(v6), Some("2001:db8::1".parse().unwrap()));
        assert_eq!(parse_ip_response("HTTP/1.1 500 Error\r\n\r\n203.0.113.7"), None);
        assert_eq!(parse_ip_response("HTTP/1.1 200 OK\r\n\r\n<html>captive portal</html>"), None);
        assert_eq!(parse_ip_response("garbage"), None);
        assert_eq!(parse_ip_response(""), None);
    }

    #[test]
    fn external_ip_works_against_a_real_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 512];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n203.0.113.9");
        });
        assert_eq!(external_ip(&url, CONNECT_TIMEOUT), Some("203.0.113.9".parse().unwrap()));
    }

    #[test]
    fn a_silent_server_cannot_hang_the_lookup() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let start = Instant::now();
        assert_eq!(external_ip(&url, Duration::from_millis(200)), None);
        assert!(start.elapsed() < Duration::from_secs(2));
        drop(listener);
    }
}
