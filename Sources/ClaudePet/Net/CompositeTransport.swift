import Foundation

/// Presents several `PeerTransport`s as one. On this branch the pet uses
/// `CompositeTransport([MultipeerLink(), LanUdpLink()])`: MultipeerConnectivity
/// still carries macOS <-> macOS, and `LanUdpLink` adds macOS <-> Windows.
///
/// ⚠️ windows-branch only — NOT part of the upstream macOS app. See CLAUDE.md.
///
/// `send` fans out to every link; each link no-ops for a peer it doesn't know,
/// so the right transport delivers. Because two Macs may discover each other on
/// *both* links, delivery messages are de-duplicated here by id before they
/// reach the pet.
final class CompositeTransport: PeerTransport {
    private let links: [PeerTransport]
    private var seenDeliveryIDs: [UUID: Date] = [:]

    var onPeersChanged: (([String]) -> Void)?
    var onReceive: ((PetMessage, String) -> Void)?

    var peerNames: [String] {
        var seen = Set<String>()
        var merged: [String] = []
        for link in links {
            for name in link.peerNames where seen.insert(name).inserted {
                merged.append(name)
            }
        }
        return merged
    }

    init(_ links: [PeerTransport]) {
        self.links = links
        for link in links {
            link.onReceive = { [weak self] message, peer in
                self?.forwardReceived(message, from: peer)
            }
            link.onPeersChanged = { [weak self] _ in
                guard let self else { return }
                self.onPeersChanged?(self.peerNames)
            }
        }
    }

    func start() {
        links.forEach { $0.start() }
    }

    func send(_ message: PetMessage, to peerName: String) {
        links.forEach { $0.send(message, to: peerName) }
    }

    private func forwardReceived(_ message: PetMessage, from peer: String) {
        if message.kind == .deliver {
            let now = Date()
            seenDeliveryIDs = seenDeliveryIDs.filter { now.timeIntervalSince($0.value) < 30 }
            if seenDeliveryIDs[message.id] != nil { return } // already handled via the other link
            seenDeliveryIDs[message.id] = now
        }
        onReceive?(message, peer)
    }
}
