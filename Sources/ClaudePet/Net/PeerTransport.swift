import Foundation

/// Abstraction over "how messages get to another machine nearby", so the pet/
/// animation code never touches MultipeerConnectivity directly. Kept narrow
/// enough that a CoreBluetooth-only implementation could be dropped in later
/// without touching anything else.
@MainActor protocol PeerTransport: AnyObject {
    /// Display names of currently-connected peers.
    var peerNames: [String] { get }

    /// Fired whenever the connected peer set changes.
    var onPeersChanged: (([String]) -> Void)? { get set }

    /// Fired when a message arrives, along with the sending peer's display name.
    var onReceive: ((PetMessage, String) -> Void)? { get set }

    /// Begins advertising/browsing for nearby peers. Safe to call once at startup.
    func start()

    /// Stops advertising/browsing and disconnects any active session, so
    /// peers see this pet drop off promptly instead of lingering in their
    /// Bonjour cache until the OS times it out - important when the app is
    /// about to quit and might be reopened again seconds later. Safe to call
    /// once at shutdown.
    func stop()

    /// Sends a message to one connected peer by display name. Silently drops
    /// the send if that peer is no longer connected.
    func send(_ message: PetMessage, to peerName: String)
}
