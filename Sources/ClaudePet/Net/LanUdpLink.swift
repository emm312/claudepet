import Foundation
import Network

/// A cross-platform `PeerTransport` over plain UDP with Bonjour discovery,
/// wire-compatible with the Windows port's mDNS + UDP transport
/// (`src-win/src/net/mdns_udp.rs`). It runs *alongside* `MultipeerLink` via
/// `CompositeTransport`, so macOS <-> macOS keeps using MultipeerConnectivity
/// while macOS <-> Windows messaging works over this link.
///
/// ⚠️ windows-branch only — NOT part of the upstream macOS app. See CLAUDE.md.
/// All Network.framework callbacks are pinned to the main queue so this file
/// stays inside the module's default `MainActor` isolation with no `nonisolated`
/// escape hatches.
final class LanUdpLink: PeerTransport {
    /// Bonjour service type. Matches the Rust side's
    /// `_claudepet._udp.local.` registration and the `NSBonjourServices` entry
    /// already in `Scripts/bundle.sh`'s Info.plist.
    private static let serviceType = "_claudepet._udp"

    private var listener: NWListener?
    private var browser: NWBrowser?

    /// Discovered peers: display name -> resolvable Bonjour endpoint.
    private var endpoints: [String: NWEndpoint] = [:]

    var onPeersChanged: (([String]) -> Void)?
    var onReceive: ((PetMessage, String) -> Void)?

    /// Same identity the MultipeerConnectivity link uses, so a peer shows up
    /// under one name regardless of which transport reached it.
    private let localName = MultipeerLink.localDisplayName

    var peerNames: [String] {
        endpoints.keys.filter { $0 != localName }.sorted()
    }

    func start() {
        startListener()
        startBrowser()
    }

    /// Tears down the listener/browser so this pet drops off the Bonjour
    /// browse promptly instead of lingering until the peer's cache expires -
    /// same rationale as `MultipeerLink.stop()`, mirrored here for the LAN
    /// link.
    func stop() {
        browser?.cancel()
        browser = nil
        listener?.cancel()
        listener = nil
    }

    // MARK: - Inbound

    private func startListener() {
        do {
            let listener = try NWListener(using: .udp)
            // Advertise over Bonjour. Apple documents setting `.service` before
            // `start()`; if the Mac never shows up in the Windows peer list,
            // switch to `NWListener(service:using:)` instead.
            listener.service = NWListener.Service(name: localName, type: Self.serviceType)
            listener.newConnectionHandler = { [weak self] connection in
                connection.start(queue: .main)
                // Network.framework doesn't statically know this closure runs
                // on the main queue; assert what's already true at runtime
                // rather than hopping with `Task` and losing callback ordering.
                MainActor.assumeIsolated {
                    self?.receiveLoop(on: connection)
                }
            }
            listener.start(queue: .main)
            self.listener = listener
        } catch {
            NSLog("ClaudePet LanUdpLink: listener failed: \(error)")
        }
    }

    private func receiveLoop(on connection: NWConnection) {
        connection.receiveMessage { [weak self] data, _, _, error in
            // Same rationale as `newConnectionHandler`: this fires on the
            // `.main` queue the connection was started with, but the compiler
            // can't see that through Network.framework's un-isolated closure.
            MainActor.assumeIsolated {
                if let data, let self {
                    self.handle(data)
                }
                if error == nil {
                    self?.receiveLoop(on: connection)
                } else {
                    connection.cancel()
                }
            }
        }
    }

    private func handle(_ data: Data) {
        guard let wire = try? JSONDecoder().decode(LanWireMessage.self, from: data),
              let message = wire.toPetMessage()
        else { return }
        onReceive?(message, message.senderName)
    }

    // MARK: - Discovery

    private func startBrowser() {
        let browser = NWBrowser(for: .bonjour(type: Self.serviceType, domain: nil), using: .udp)
        browser.browseResultsChangedHandler = { [weak self] results, _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                var map: [String: NWEndpoint] = [:]
                for result in results {
                    if case let .service(name, _, _, _) = result.endpoint {
                        map[name] = result.endpoint
                    }
                }
                self.endpoints = map
                self.onPeersChanged?(self.peerNames)
            }
        }
        browser.start(queue: .main)
        self.browser = browser
    }

    // MARK: - Outbound

    func send(_ message: PetMessage, to peerName: String) {
        guard let endpoint = endpoints[peerName] else {
            NSLog("ClaudePet LanUdpLink: send to unknown peer \(peerName)")
            return
        }
        guard let data = try? JSONEncoder().encode(LanWireMessage(message)) else { return }

        // `endpoint` is an unresolved Bonjour `.service` endpoint - the browser only
        // ever enumerated its name, it never resolved host/port. Sending immediately
        // after `start()` (the old behavior) fired the send while the connection was
        // still `.preparing` mid-resolution, so the datagram was silently dropped
        // instead of queued - discovery worked but delivery never did. Wait for
        // `.ready` (resolution complete) before handing off data, and log every
        // failure path since none of them surfaced anywhere before.
        let connection = NWConnection(to: endpoint, using: .udp)
        connection.stateUpdateHandler = { [weak connection] state in
            MainActor.assumeIsolated {
                guard let connection else { return }
                switch state {
                case .ready:
                    connection.send(content: data, completion: .contentProcessed { error in
                        if let error {
                            NSLog("ClaudePet LanUdpLink: send to \(peerName) failed: \(error)")
                        }
                        connection.cancel()
                    })
                case .failed(let error):
                    NSLog("ClaudePet LanUdpLink: connection to \(peerName) failed: \(error)")
                    connection.cancel()
                case .waiting(let error):
                    NSLog("ClaudePet LanUdpLink: connection to \(peerName) waiting: \(error)")
                default:
                    break
                }
            }
        }
        connection.start(queue: .main)
    }
}

/// The exact JSON shape the Windows port speaks: a flat object with lowercase
/// `kind`/`exitEdge`, a lowercase dashed-UUID `id`, and `sentAt` as Unix
/// seconds. Kept separate from `PetMessage` so the MultipeerConnectivity path's
/// `Codable` encoding is untouched.
private struct LanWireMessage: Codable {
    let id: String
    let kind: String
    let text: String
    let senderName: String
    let exitEdge: String
    let sentAt: Double
    let express: Bool

    init(_ message: PetMessage) {
        id = message.id.uuidString.lowercased()
        kind = message.kind.rawValue
        text = message.text
        senderName = message.senderName
        exitEdge = message.exitEdge.rawValue
        sentAt = message.sentAt.timeIntervalSince1970
        express = message.express
    }

    func toPetMessage() -> PetMessage? {
        guard let uuid = UUID(uuidString: id),
              let kind = PetMessage.Kind(rawValue: kind),
              let edge = PetMessage.Edge(rawValue: exitEdge)
        else { return nil }
        return PetMessage(
            id: uuid,
            kind: kind,
            text: text,
            senderName: senderName,
            exitEdge: edge,
            sentAt: Date(timeIntervalSince1970: sentAt),
            express: express
        )
    }
}
