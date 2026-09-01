//! Pet-to-pet messaging: the wire message + the transport abstraction.
//! Mirrors `Sources/ClaudePet/Net/PetMessage.swift` and `Net/PeerTransport.swift`.
//!
//! The Swift `PeerTransport` protocol is callback-based (`onReceive`,
//! `onPeersChanged`). This port is poll-based instead - the runtime already has
//! a tick loop, so it drains `try_recv()` and diffs `peer_names()` each tick,
//! which avoids threading closures across the mDNS/UDP background threads.

pub mod mdns_udp;

use serde::{Deserialize, Serialize};

/// Which edge of the sender's screen the pet exited through. The receiver spawns
/// its visitor on the opposite edge so the trip reads as continuous.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Left,
    Right,
}

impl Edge {
    pub fn opposite(self) -> Edge {
        match self {
            Edge::Left => Edge::Right,
            Edge::Right => Edge::Left,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Deliver,
    Ack,
}

/// Wire format for pet-to-pet messages, JSON-encoded over a `PeerTransport`.
/// Field names match the Swift struct (`senderName`, `exitEdge`, `sentAt`) so the
/// payload shape is identical; `sentAt` is plain Unix seconds here rather than
/// Foundation's reference-date encoding (cross-platform messaging is not a goal).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PetMessage {
    pub id: String,
    pub kind: Kind,
    /// empty for `Ack`
    pub text: String,
    #[serde(rename = "senderName")]
    pub sender_name: String,
    #[serde(rename = "exitEdge")]
    pub exit_edge: Edge,
    #[serde(rename = "sentAt")]
    pub sent_at: f64,
}

impl PetMessage {
    pub fn deliver(text: String, sender_name: String, exit_edge: Edge) -> PetMessage {
        PetMessage {
            id: random_id(),
            kind: Kind::Deliver,
            text,
            sender_name,
            exit_edge,
            sent_at: crate::pet::pet_state::now_secs(),
        }
    }

    /// The ack for a given delivery - correlates by `id` so a stray/duplicate ack
    /// can't resolve the wrong outbound courier.
    pub fn make_ack(&self) -> PetMessage {
        PetMessage {
            id: self.id.clone(),
            kind: Kind::Ack,
            text: String::new(),
            sender_name: self.sender_name.clone(),
            exit_edge: self.exit_edge,
            sent_at: crate::pet::pet_state::now_secs(),
        }
    }
}

/// A random lowercase UUIDv4 string. Windows only ever compares it for equality,
/// but the canonical `8-4-4-4-12` shape lets the macOS side round-trip it through
/// Foundation's `UUID` without a lossy remap (see `Net/LanUdpLink.swift` on the
/// windows branch).
pub fn random_id() -> String {
    let mut b: [u8; 16] = rand::random();
    b[6] = (b[6] & 0x0f) | 0x40; // version 4
    b[8] = (b[8] & 0x3f) | 0x80; // variant 1
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    )
}

/// "How messages get to another machine nearby", so the pet/animation code never
/// touches the discovery/socket layer directly.
pub trait PeerTransport: Send {
    /// Begin advertising/browsing for nearby peers. Safe to call once at startup.
    fn start(&mut self);

    /// This instance's own display name.
    fn local_name(&self) -> String;

    /// Display names of currently-connected/known peers.
    fn peer_names(&self) -> Vec<String>;

    /// Send a message to one peer by display name. Silently drops the send if
    /// that peer is unknown.
    fn send(&self, message: &PetMessage, to_peer: &str);

    /// Pop one received message (with the sending peer's display name), if any.
    fn try_recv(&self) -> Option<(PetMessage, String)>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_opposite() {
        assert_eq!(Edge::Left.opposite(), Edge::Right);
        assert_eq!(Edge::Right.opposite(), Edge::Left);
    }

    #[test]
    fn random_id_is_a_lowercase_dashed_uuid() {
        let id = random_id();
        assert_eq!(id.len(), 36);
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.iter().map(|p| p.len()).collect::<Vec<_>>(), vec![8, 4, 4, 4, 12]);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase() || c == '-'));
        assert_eq!(&id[14..15], "4"); // version nibble
        assert_ne!(random_id(), random_id());
    }

    #[test]
    fn ack_preserves_id_and_clears_text() {
        let m = PetMessage::deliver("hi there".into(), "DeskA".into(), Edge::Right);
        let ack = m.make_ack();
        assert_eq!(ack.id, m.id);
        assert_eq!(ack.kind, Kind::Ack);
        assert!(ack.text.is_empty());
        assert_eq!(ack.sender_name, "DeskA");
        assert_eq!(ack.exit_edge, Edge::Right);
    }

    #[test]
    fn json_round_trips_with_swift_field_names() {
        let m = PetMessage::deliver("ship it".into(), "DeskB".into(), Edge::Left);
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"senderName\":\"DeskB\""));
        assert!(json.contains("\"exitEdge\":\"left\""));
        assert!(json.contains("\"kind\":\"deliver\""));
        let back: PetMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, m.id);
        assert_eq!(back.text, "ship it");
        assert_eq!(back.exit_edge, Edge::Left);
    }
}
