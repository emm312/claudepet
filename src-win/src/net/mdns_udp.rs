//! `PeerTransport` backed by mDNS service discovery (`mdns-sd`) plus plain UDP
//! datagrams for the `PetMessage` payloads. Replaces the macOS
//! `Sources/ClaudePet/Net/MultipeerLink.swift`, which used Apple's
//! MultipeerConnectivity (no Windows equivalent).
//!
//! Discovery: advertise + browse `_claudepet._udp.local.`. Each instance's
//! advertised name is its display name, and its SRV record carries the UDP port
//! its socket is bound to. Messaging: JSON-encoded `PetMessage` sent to the
//! resolved `ip:port` of the target peer.

use super::{PeerTransport, PetMessage};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use socket2::{Domain, Socket, Type};
use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, Ipv6Addr, SocketAddr, SocketAddrV6, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

const SERVICE_TYPE: &str = "_claudepet._udp.local.";
/// How long a source address learned from an inbound datagram is trusted over
/// a freshly mDNS-resolved advertised address.
const LEARNED_TTL: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// A peer's known addresses. `learned` (the source address of its most recent
/// datagram to us) is preferred while fresh - it's proven reachable right now,
/// whereas `advertised` (from mDNS `ServiceResolved`) can be a virtual
/// adapter's address on a multi-homed host, or briefly overwrite a
/// known-working address with one that isn't. Kept as two fields, rather than
/// clobbering one on every resolve, so a resolve can never regress a working
/// send/ack loop.
#[derive(Clone, Copy, Default)]
struct PeerAddrs {
    advertised: Option<SocketAddr>,
    learned: Option<(SocketAddr, Instant)>,
}

impl PeerAddrs {
    fn best(&self) -> Option<SocketAddr> {
        if let Some((addr, at)) = self.learned {
            if at.elapsed() < LEARNED_TTL {
                return Some(addr);
            }
        }
        self.advertised
    }
}

pub struct MdnsUdpTransport {
    local_name: String,
    socket: UdpSocket,
    udp_port: u16,
    peers: Arc<Mutex<HashMap<String, PeerAddrs>>>,
    inbox: Arc<Mutex<VecDeque<(PetMessage, String)>>>,
    mdns: Option<ServiceDaemon>,
    /// Bumped by `rescan()`; the browse worker re-issues its own `browse()`
    /// when it sees the generation change. mdns-sd keys queriers by service
    /// type, so a second browse from anywhere else would *replace* the long
    /// lived subscription - this counter lets an active re-scan refresh the
    /// peer map without ever stealing it.
    scan_generation: Arc<AtomicU64>,
    /// Set by `stop()`; the browse worker polls this and exits promptly rather
    /// than re-browsing forever against a shut-down daemon.
    shutdown: Arc<AtomicBool>,
    started: bool,
}

/// Local display name. Overridable via `CLAUDEPET_PEER_NAME` so two instances can
/// run on one box during development (matches the Swift override).
pub fn local_display_name() -> String {
    if let Ok(name) = std::env::var("CLAUDEPET_PEER_NAME") {
        if !name.is_empty() {
            return name;
        }
    }
    if let Ok(host) = std::env::var("COMPUTERNAME") {
        if !host.is_empty() {
            return host;
        }
    }
    format!("ClaudePet-{}", rand::random::<u16>() % 9000 + 1000)
}

impl MdnsUdpTransport {
    pub fn new() -> std::io::Result<Self> {
        // A dual-stack IPv6 socket (IPV6_V6ONLY off) rather than a plain IPv4
        // bind: mDNS resolution of a macOS peer can hand back an IPv6 address,
        // and an IPv4-only socket silently never sees those datagrams even
        // though discovery (which only exchanges names, not a live send)
        // succeeds - Windows could see a Mac in "Search for pets" but never
        // receive a message from it. `send()` below maps any IPv4 peer
        // address to its IPv4-mapped IPv6 form so this one socket still
        // reaches IPv4-only peers (e.g. other Windows instances) too.
        let socket2 = Socket::new(Domain::IPV6, Type::DGRAM, None)?;
        socket2.set_only_v6(false)?;
        socket2.bind(&SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0).into())?;
        let socket: UdpSocket = socket2.into();
        let udp_port = socket.local_addr()?.port();
        Ok(MdnsUdpTransport {
            local_name: local_display_name(),
            socket,
            udp_port,
            peers: Arc::new(Mutex::new(HashMap::new())),
            inbox: Arc::new(Mutex::new(VecDeque::new())),
            mdns: None,
            scan_generation: Arc::new(AtomicU64::new(0)),
            shutdown: Arc::new(AtomicBool::new(false)),
            started: false,
        })
    }

    fn spawn_recv_thread(&self) {
        let Ok(sock) = self.socket.try_clone() else {
            return;
        };
        let peers = Arc::clone(&self.peers);
        let inbox = Arc::clone(&self.inbox);
        let own_name = self.local_name.clone();
        let shutdown = Arc::clone(&self.shutdown);
        // A 500ms read timeout, not a blocking recv, so this thread notices
        // `shutdown` (set by `stop()`) instead of blocking forever - previously
        // there was no way to exit it, so a stop()->start() cycle doubled it.
        let _ = sock.set_read_timeout(Some(std::time::Duration::from_millis(500)));
        std::thread::spawn(move || {
            let mut buf = [0u8; 64 * 1024];
            while !shutdown.load(Ordering::SeqCst) {
                match sock.recv_from(&mut buf) {
                    Ok((n, from)) => {
                        if let Ok(msg) = serde_json::from_slice::<PetMessage>(&buf[..n]) {
                            let sender = msg.sender_name.clone();
                            // Inbound datagrams are proof the sender is alive
                            // and reachable *right now* - remember the source
                            // address so replies (acks, follow-ups) go straight
                            // to a working socket even if the mDNS SRV record
                            // resolved to a different IP, or none at all (e.g.
                            // a peer advertising only unscoped link-local v6).
                            // `recv_from` fills in the scope id the OS used,
                            // which mDNS resolution never provides. Never
                            // record our own name - an ack we sent ourselves
                            // (loopback discovery quirks) must not show up as
                            // a peer.
                            if sender != own_name {
                                let mut map = peers.lock().unwrap();
                                let entry = map.entry(sender.clone()).or_default();
                                entry.learned = Some((from, Instant::now()));
                            }
                            inbox.lock().unwrap().push_back((msg, sender));
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut || e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => {
                        std::thread::sleep(std::time::Duration::from_millis(200));
                    }
                }
            }
        });
    }

    fn spawn_mdns_thread(&mut self) -> std::io::Result<()> {
        let mdns = ServiceDaemon::new()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

        let host_name = format!(
            "{}.local.",
            std::env::var("COMPUTERNAME").unwrap_or_else(|_| "claudepet-host".into())
        );
        let service = ServiceInfo::new(
            SERVICE_TYPE,
            &self.local_name,
            &host_name,
            "",
            self.udp_port,
            None,
        )
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?
        .enable_addr_auto();
        let _ = mdns.register(service);

        // Prove the daemon is usable before returning; the worker below owns its
        // own (re-)subscription.
        mdns.browse(SERVICE_TYPE)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

        let peers = Arc::clone(&self.peers);
        let own_name = self.local_name.clone();
        let scan_generation = Arc::clone(&self.scan_generation);
        let shutdown = Arc::clone(&self.shutdown);
        let worker_mdns = mdns.clone();
        std::thread::spawn(move || {
            // This worker is the *sole* owner of the `_claudepet._udp` browse.
            // mdns-sd keeps one querier per service type (`service_queriers`),
            // so any other `browse()` - like the old one-shot rescan - replaces
            // this subscription and kills passive discovery. `rescan()` only
            // bumps `scan_generation`; when the worker sees the bump it re-issues
            // its own browse, which re-sends the PTR query and replays the
            // daemon's cache of known peers (`query_cache_for_service`), so a
            // "Search for pets" click refreshes the map without stealing the slot.
            let mut last_generation = scan_generation.load(Ordering::SeqCst);
            while !shutdown.load(Ordering::SeqCst) {
                let Ok(receiver) = worker_mdns.browse(SERVICE_TYPE) else {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    continue;
                };
                loop {
                    match receiver.recv_timeout(std::time::Duration::from_millis(500)) {
                        Ok(ServiceEvent::ServiceResolved(info)) => {
                            let name = instance_label(info.get_fullname());
                            if name == own_name {
                                continue;
                            }
                            if let Some(addr) = pick_peer_addr(&info) {
                                // Only the advertised slot - never clobber a
                                // `learned` address proven reachable by actual
                                // traffic (see `PeerAddrs`).
                                peers.lock().unwrap().entry(name).or_default().advertised = Some(addr);
                            }
                        }
                        Ok(ServiceEvent::ServiceRemoved(_ty, fullname)) => {
                            let name = instance_label(&fullname);
                            peers.lock().unwrap().remove(&name);
                        }
                        Ok(_) => {}
                        Err(flume::RecvTimeoutError::Disconnected) => break,
                        Err(flume::RecvTimeoutError::Timeout) => {
                            // Nothing arrived in the poll window. If a rescan
                            // bumped the generation, re-issue the browse so
                            // the daemon replays its cache immediately.
                            let generation = scan_generation.load(Ordering::SeqCst);
                            if generation != last_generation {
                                last_generation = generation;
                                break;
                            }
                        }
                    }
                }
            }
        });

        self.mdns = Some(mdns);
        Ok(())
    }
}

/// `MyName._claudepet._udp.local.` -> `MyName`
fn instance_label(fullname: &str) -> String {
    fullname
        .strip_suffix(&format!(".{SERVICE_TYPE}"))
        .unwrap_or(fullname)
        .to_string()
}

/// Pick the best reachable address for a resolved peer. IPv4 first - it travels
/// fine through the dual-stack socket via the IPv4-mapped form `send()` uses.
/// Among IPv6 candidates, skip link-local (fe80::/10) addresses: mDNS
/// resolution never carries the interface scope id those need, so `send_to`
/// fails with EINVAL. (A peer that only advertises such an address still ends
/// up in the peer map the first time it sends *us* a datagram - `recv_from`
/// does carry the scope id.) Prefer a non-link-local v6 as the fallback.
fn pick_peer_addr(info: &ServiceInfo) -> Option<SocketAddr> {
    let port = info.get_port();
    let mut v6_fallback: Option<SocketAddr> = None;
    for ip in info.get_addresses() {
        match ip {
            IpAddr::V4(_) => return Some(SocketAddr::new(*ip, port)),
            IpAddr::V6(v6) => {
                if !v6.is_unicast_link_local() && v6_fallback.is_none() {
                    v6_fallback = Some(SocketAddr::new(*ip, port));
                }
            }
        }
    }
    v6_fallback
}

impl PeerTransport for MdnsUdpTransport {
    fn start(&mut self) {
        if self.started {
            return;
        }
        self.started = true;
        // Allow a stop() -> start() cycle (the shutdown flag is sticky by
        // design so the old worker thread exits before a new daemon is spun up).
        self.shutdown.store(false, Ordering::SeqCst);
        self.spawn_recv_thread();
        if let Err(e) = self.spawn_mdns_thread() {
            eprintln!("ClaudePet: mDNS discovery unavailable: {e}");
        }
    }

    fn stop(&mut self) {
        if !self.started {
            return;
        }
        self.started = false;
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(mdns) = &self.mdns {
            // Graceful GOODBYE so peers drop us from their mDNS cache
            // immediately instead of holding the stale `_claudepet._udp` entry
            // until the PTR TTL (~75 min) expires. Without this, a quick
            // quit-then-relaunch leaves the same-named new instance invisible
            // to peers whose cache still points at the dead one - the exact
            // "closed it and it never shows up again" bug.
            let fullname = format!("{}.{SERVICE_TYPE}", self.local_name);
            let _ = mdns.unregister(&fullname);
            let _ = mdns.shutdown();
        }
        self.mdns = None;
    }

    fn local_name(&self) -> String {
        self.local_name.clone()
    }

    fn peer_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .peers
            .lock()
            .unwrap()
            .keys()
            .filter(|n| **n != self.local_name)
            .cloned()
            .collect();
        names.sort();
        names
    }

    fn send(&self, message: &PetMessage, to_peer: &str) {
        // Unknown peer -> ask the browse worker for a refresh; the caller
        // (`Runtime`, at `Away` entry) retries shortly after rather than this
        // call blocking the UI thread on a fixed sleep.
        let addr = self.peers.lock().unwrap().get(to_peer).and_then(PeerAddrs::best);
        let Some(addr) = addr else {
            self.rescan();
            eprintln!("ClaudePet: dropping message to unknown peer {to_peer}");
            return;
        };
        // The socket is IPv6-only at the API level (dual-stack via
        // IPV6_V6ONLY=false happens under the hood, but Rust's std still
        // requires the SocketAddr passed in to be V6) - map a resolved IPv4
        // peer address to its IPv4-mapped IPv6 form rather than dropping it.
        let addr = match addr {
            SocketAddr::V4(v4) => SocketAddr::V6(SocketAddrV6::new(
                v4.ip().to_ipv6_mapped(),
                v4.port(),
                0,
                0,
            )),
            v6 => v6,
        };
        if let Ok(bytes) = serde_json::to_vec(message) {
            if let Err(e) = self.socket.send_to(&bytes, addr) {
                eprintln!("ClaudePet: send to {to_peer} ({addr}) failed: {e}");
            }
        }
    }

    fn try_recv(&self) -> Option<(PetMessage, String)> {
        self.inbox.lock().unwrap().pop_front()
    }

    /// Ask the browse worker to refresh the peer map. mdns-sd keys queriers by
    /// service type, so a direct `browse()` here would *replace* the worker's
    /// long-lived subscription (and `stop_browse` would kill it entirely) -
    /// that was the old bug where passive discovery died after the first
    /// "Search for pets" click. Bumping the generation instead makes the worker
    /// re-issue its own browse within ~500ms, which replays the daemon's cache
    /// of known peers.
    fn rescan(&self) {
        self.scan_generation.fetch_add(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::Edge;

    #[test]
    fn instance_label_strips_service_suffix() {
        assert_eq!(instance_label("DeskA._claudepet._udp.local."), "DeskA");
        assert_eq!(instance_label("bare"), "bare");
    }

    #[test]
    fn udp_round_trip_over_loopback() {
        // Two transports, wired directly by address (skipping mDNS) to prove the
        // JSON-over-UDP message path.
        let a = MdnsUdpTransport::new().unwrap();
        let b = MdnsUdpTransport::new().unwrap();
        a.start_recv_only();
        b.start_recv_only();

        let b_addr: SocketAddr = format!("127.0.0.1:{}", b.udp_port).parse().unwrap();
        a.peers.lock().unwrap().insert(
            "B".into(),
            PeerAddrs { advertised: Some(b_addr), learned: None },
        );

        let msg = PetMessage::deliver("ping".into(), "A".into(), Edge::Right, false);
        a.send(&msg, "B");

        // Give the recv thread a moment.
        let mut got = None;
        for _ in 0..50 {
            if let Some(x) = b.try_recv() {
                got = Some(x);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let (recv_msg, sender) = got.expect("message not received");
        assert_eq!(recv_msg.text, "ping");
        assert_eq!(recv_msg.id, msg.id);
        assert_eq!(sender, "A");
    }

    impl MdnsUdpTransport {
        /// Test helper: start only the UDP receive loop, not mDNS.
        fn start_recv_only(&self) {
            self.spawn_recv_thread();
        }
    }
}
