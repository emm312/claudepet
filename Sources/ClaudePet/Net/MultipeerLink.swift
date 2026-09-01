import Foundation
import MultipeerConnectivity

/// `PeerTransport` backed by MultipeerConnectivity, which automatically picks
/// Bluetooth or peer-to-peer Wi-Fi depending on what's available - we never
/// choose the radio ourselves.
///
/// Service type must be <=15 chars, lowercase, hyphens only: "claudepet".
final class MultipeerLink: NSObject, PeerTransport {
    private static let serviceType = "claudepet"

    /// Overridable via `CLAUDEPET_PEER_NAME` so two instances can run on one
    /// Mac during development without colliding on the same display name.
    static var localDisplayName: String {
        if let override = ProcessInfo.processInfo.environment["CLAUDEPET_PEER_NAME"], !override.isEmpty {
            return override
        }
        return Host.current().localizedName ?? "ClaudePet-\(Int.random(in: 1000..<9999))"
    }

    // MultipeerConnectivity's session/advertiser/browser are documented as
    // safe to call from any thread, and their delegate callbacks always land
    // off the main thread regardless of what queue we ask for - so these are
    // deliberately exempted from the module's default MainActor isolation
    // rather than bounced through a Task for every call.
    private nonisolated(unsafe) let peerID: MCPeerID
    private nonisolated(unsafe) let session: MCSession
    private nonisolated(unsafe) let advertiser: MCNearbyServiceAdvertiser
    private nonisolated(unsafe) let browser: MCNearbyServiceBrowser

    var onPeersChanged: (([String]) -> Void)?
    var onReceive: ((PetMessage, String) -> Void)?

    nonisolated var peerNames: [String] {
        session.connectedPeers.map { $0.displayName }
    }

    override init() {
        let peerID = MCPeerID(displayName: Self.localDisplayName)
        self.peerID = peerID
        session = MCSession(peer: peerID, securityIdentity: nil, encryptionPreference: .optional)
        advertiser = MCNearbyServiceAdvertiser(peer: peerID, discoveryInfo: nil, serviceType: Self.serviceType)
        browser = MCNearbyServiceBrowser(peer: peerID, serviceType: Self.serviceType)
        super.init()
        session.delegate = self
        advertiser.delegate = self
        browser.delegate = self
    }

    func start() {
        advertiser.startAdvertisingPeer()
        browser.startBrowsingForPeers()
    }

    func send(_ message: PetMessage, to peerName: String) {
        guard let peer = session.connectedPeers.first(where: { $0.displayName == peerName }),
              let data = try? JSONEncoder().encode(message)
        else { return }
        try? session.send(data, toPeers: [peer], with: .reliable)
    }

    /// Only the lexicographically-lower display name invites the other side -
    /// letting both sides invite each other is the classic Multipeer bug that
    /// produces duplicate/flapping sessions between the same two peers.
    private nonisolated func shouldInvite(_ other: MCPeerID) -> Bool {
        peerID.displayName < other.displayName
    }
}

extension MultipeerLink: MCSessionDelegate {
    nonisolated func session(_ session: MCSession, peer peerID: MCPeerID, didChange state: MCSessionState) {
        let names = self.peerNames
        Task { @MainActor in
            self.onPeersChanged?(names)
        }
    }

    nonisolated func session(_ session: MCSession, didReceive data: Data, fromPeer peerID: MCPeerID) {
        let displayName = peerID.displayName
        Task { @MainActor in
            guard let message = try? JSONDecoder().decode(PetMessage.self, from: data) else { return }
            self.onReceive?(message, displayName)
        }
    }

    nonisolated func session(_ session: MCSession, didReceive stream: InputStream, withName streamName: String, fromPeer peerID: MCPeerID) {}
    nonisolated func session(_ session: MCSession, didStartReceivingResourceWithName resourceName: String, fromPeer peerID: MCPeerID, with progress: Progress) {}
    nonisolated func session(_ session: MCSession, didFinishReceivingResourceWithName resourceName: String, fromPeer peerID: MCPeerID, at localURL: URL?, withError error: Error?) {}
}

extension MultipeerLink: MCNearbyServiceAdvertiserDelegate {
    nonisolated func advertiser(_ advertiser: MCNearbyServiceAdvertiser, didReceiveInvitationFromPeer peerID: MCPeerID, withContext context: Data?, invitationHandler: @escaping (Bool, MCSession?) -> Void) {
        // Auto-accept every invitation - there's no untrusted-peer story here yet.
        invitationHandler(true, session)
    }
}

extension MultipeerLink: MCNearbyServiceBrowserDelegate {
    nonisolated func browser(_ browser: MCNearbyServiceBrowser, foundPeer peerID: MCPeerID, withDiscoveryInfo info: [String: String]?) {
        guard shouldInvite(peerID) else { return }
        browser.invitePeer(peerID, to: session, withContext: nil, timeout: 15)
    }

    nonisolated func browser(_ browser: MCNearbyServiceBrowser, lostPeer peerID: MCPeerID) {
        let names = self.peerNames
        Task { @MainActor in
            self.onPeersChanged?(names)
        }
    }
}
