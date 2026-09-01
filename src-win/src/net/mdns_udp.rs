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
use std::collections::{HashMap, VecDeque};
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};

const SERVICE_TYPE: &str = "_claudepet._udp.local.";

pub struct MdnsUdpTransport {
    local_name: String,
    socket: UdpSocket,
    udp_port: u16,
    peers: Arc<Mutex<HashMap<String, SocketAddr>>>,
    inbox: Arc<Mutex<VecDeque<(PetMessage, String)>>>,
    mdns: Option<ServiceDaemon>,
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
        let socket = UdpSocket::bind(("0.0.0.0", 0))?;
        let udp_port = socket.local_addr()?.port();
        Ok(MdnsUdpTransport {
            local_name: local_display_name(),
            socket,
            udp_port,
            peers: Arc::new(Mutex::new(HashMap::new())),
            inbox: Arc::new(Mutex::new(VecDeque::new())),
            mdns: None,
            started: false,
        })
    }

    fn spawn_recv_thread(&self) {
        let Ok(sock) = self.socket.try_clone() else {
            return;
        };
        let inbox = Arc::clone(&self.inbox);
        std::thread::spawn(move || {
            let mut buf = [0u8; 64 * 1024];
            loop {
                match sock.recv_from(&mut buf) {
                    Ok((n, _from)) => {
                        if let Ok(msg) = serde_json::from_slice::<PetMessage>(&buf[..n]) {
                            let sender = msg.sender_name.clone();
                            inbox.lock().unwrap().push_back((msg, sender));
                        }
                    }
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

        let receiver = mdns
            .browse(SERVICE_TYPE)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

        let peers = Arc::clone(&self.peers);
        let own_name = self.local_name.clone();
        std::thread::spawn(move || {
            while let Ok(event) = receiver.recv() {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        let name = instance_label(info.get_fullname());
                        if name == own_name {
                            continue;
                        }
                        let port = info.get_port();
                        if let Some(ip) = info.get_addresses().iter().next() {
                            let addr = SocketAddr::new(*ip, port);
                            peers.lock().unwrap().insert(name, addr);
                        }
                    }
                    ServiceEvent::ServiceRemoved(_ty, fullname) => {
                        let name = instance_label(&fullname);
                        peers.lock().unwrap().remove(&name);
                    }
                    _ => {}
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

impl PeerTransport for MdnsUdpTransport {
    fn start(&mut self) {
        if self.started {
            return;
        }
        self.started = true;
        self.spawn_recv_thread();
        if let Err(e) = self.spawn_mdns_thread() {
            eprintln!("ClaudePet: mDNS discovery unavailable: {e}");
        }
    }

    fn local_name(&self) -> String {
        self.local_name.clone()
    }

    fn peer_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.peers.lock().unwrap().keys().cloned().collect();
        names.sort();
        names
    }

    fn send(&self, message: &PetMessage, to_peer: &str) {
        let addr = self.peers.lock().unwrap().get(to_peer).copied();
        let Some(addr) = addr else { return };
        if let Ok(bytes) = serde_json::to_vec(message) {
            let _ = self.socket.send_to(&bytes, addr);
        }
    }

    fn try_recv(&self) -> Option<(PetMessage, String)> {
        self.inbox.lock().unwrap().pop_front()
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
        a.peers.lock().unwrap().insert("B".into(), b_addr);

        let msg = PetMessage::deliver("ping".into(), "A".into(), Edge::Right);
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
