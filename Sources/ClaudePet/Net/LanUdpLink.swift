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
    /// The listener's bound UDP port, once known. Every outbound `NWConnection`
    /// is pinned to originate from this same port (`outboundParameters`) - see
    /// the note on `send` below for why that matters.
    private var listenerPort: NWEndpoint.Port?

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
            listener.stateUpdateHandler = { [weak self] state in
                MainActor.assumeIsolated {
                    guard let self else { return }
                    switch state {
                    case .ready:
                        self.listenerPort = listener.port
                    case .failed, .cancelled:
                        // Without a restart here, a listener that dies after
                        // start() (sleep/wake, interface change) left the pet
                        // silently unable to receive anything - forever, with
                        // no user-visible sign - while the browser side
                        // already had this self-heal.
                        guard self.listener === listener else { return }
                        self.listener = nil
                        DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) { [weak self] in
                            self?.startListener()
                        }
                    default:
                        break
                    }
                }
            }
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
                    // The message's source address isn't in the completion -
                    // read it off the connection. Inbound datagrams are proof
                    // the sender is alive and reachable *right now*, so
                    // remember its endpoint for replies: covers the case where
                    // Bonjour resolution never completed, or resolved to a
                    // different address than the one traffic actually flows on.
                    self.handle(data, from: connection.endpoint)
                }
                if error == nil {
                    self?.receiveLoop(on: connection)
                } else {
                    connection.cancel()
                }
            }
        }
    }

    private func handle(_ data: Data, from endpoint: NWEndpoint?) {
        guard let wire = try? JSONDecoder().decode(LanWireMessage.self, from: data),
              let message = wire.toPetMessage()
        else { return }
        if let endpoint, endpoints[message.senderName] != endpoint {
            endpoints[message.senderName] = endpoint
            onPeersChanged?(peerNames)
        }
        onReceive?(message, message.senderName)
    }

    // MARK: - Discovery

    private func startBrowser() {
        let browser = NWBrowser(for: .bonjour(type: Self.serviceType, domain: nil), using: .udp)
        browser.browseResultsChangedHandler = { [weak self] results, _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                var seen = Set<String>()
                for result in results {
                    if case let .service(name, _, _, _) = result.endpoint {
                        seen.insert(name)
                        // Only fill in peers we don't already have an endpoint
                        // for - never overwrite one learned from a live
                        // datagram's source address (`handle(_:from:)`), which
                        // is proven reachable, with the browse's unresolved
                        // advertised endpoint.
                        if self.endpoints[name] == nil {
                            self.endpoints[name] = result.endpoint
                        }
                    }
                }
                // Drop only peers the browse no longer sees at all - a
                // snapshot replace here would also discard an endpoint learned
                // from a live datagram's source address whenever a routine
                // browse refresh fires in between.
                self.endpoints = self.endpoints.filter { seen.contains($0.key) }
                self.onPeersChanged?(self.peerNames)
            }
        }
        browser.stateUpdateHandler = { [weak self] state in
            MainActor.assumeIsolated {
                guard let self else { return }
                switch state {
                case .failed, .cancelled:
                    // A browser can die silently - wake-from-sleep, an
                    // interface change, a transient failure - which freezes the
                    // peer list at whatever it last saw and is the classic
                    // "closed and reopened, never shows up again" cause.
                    // Re-create it so passive discovery resumes. Only restart
                    // if this is still the live instance: `stop()` cancels and
                    // clears `self.browser`, and that shutdown `.cancelled`
                    // must not spin the browser back up.
                    guard self.browser === browser else { return }
                    self.browser = nil
                    DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) { [weak self] in
                        self?.startBrowser()
                    }
                default:
                    break
                }
            }
        }
        browser.start(queue: .main)
        self.browser = browser
    }

    // MARK: - Outbound

    func send(_ message: PetMessage, to peerName: String) {
        guard let endpoint = endpoints[peerName] else {
            // Unknown peer: the passive browse can lag reality (peer just
            // relaunched, browser re-subscribing after a failure). Restart
            // discovery once and retry after a beat before giving up -
            // silently dropping here was the old behavior and produced
            // messages that vanished with no bubble at all.
            NSLog("ClaudePet LanUdpLink: send to unknown peer \(peerName) - restarting browse")
            restartBrowserForRetry()
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) { [weak self] in
                guard let self, self.endpoints[peerName] != nil else {
                    NSLog("ClaudePet LanUdpLink: peer \(peerName) still unknown after browse restart")
                    return
                }
                self.send(message, to: peerName)
            }
            return
        }
        guard let data = try? JSONEncoder().encode(LanWireMessage(message)) else { return }

        // Every outbound datagram (deliver, ack) must leave from the *same*
        // source port the listener is bound to - otherwise each `NWConnection`
        // below picks a fresh ephemeral port, and the peer's "upsert the source
        // address of every inbound datagram" reachability tracking
        // (`handle(_:from:)` here, the Rust side's `mdns_udp.rs` recv thread)
        // keeps overwriting a working address with one that's already closed.
        // That was the primary cause of acks silently vanishing.
        let params = NWParameters.udp
        if let port = listenerPort {
            params.requiredLocalEndpoint = .hostPort(host: "::", port: port)
            params.allowLocalEndpointReuse = true
        }

        // `endpoint` is an unresolved Bonjour `.service` endpoint - the browser only
        // ever enumerated its name, it never resolved host/port. Sending immediately
        // after `start()` (the old behavior) fired the send while the connection was
        // still `.preparing` mid-resolution, so the datagram was silently dropped
        // instead of queued - discovery worked but delivery never did. Wait for
        // `.ready` (resolution complete) before handing off data, and log every
        // failure path since none of them surfaced anywhere before.
        let connection = NWConnection(to: endpoint, using: params)
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

    private func restartBrowserForRetry() {
        browser?.cancel()
        browser = nil
        startBrowser()
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
