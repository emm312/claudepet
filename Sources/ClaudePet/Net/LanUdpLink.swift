import Foundation
import Network
#if canImport(Darwin)
import Darwin
#endif

/// A cross-platform `PeerTransport` over plain UDP with Bonjour discovery,
/// wire-compatible with the Windows port's mDNS + UDP transport
/// (`src-win/src/net/mdns_udp.rs`). It runs *alongside* `MultipeerLink` via
/// `CompositeTransport`, so macOS <-> macOS keeps using MultipeerConnectivity
/// while macOS <-> Windows messaging works over this link.
///
/// ⚠️ windows-branch only — NOT part of the upstream macOS app. See CLAUDE.md.
///
/// Sending and receiving both go through **one raw BSD UDP socket** (dual-stack
/// IPv6, `IPV6_V6ONLY` off), mirroring the Windows port's `mdns_udp.rs` exactly.
/// An earlier version of this file sent each datagram through its own
/// short-lived `NWConnection`, which meant every send left from a fresh
/// ephemeral port - a peer's "the address this pet last talked to us from"
/// tracking then pointed at a socket that was already closed by the time an
/// ack came back, so acks were lost. A later attempt pinned outbound
/// `NWConnection`s to the listener's port via `requiredLocalEndpoint` +
/// `allowLocalEndpointReuse`, but Network.framework does not reliably support
/// sharing a UDP port between an active `NWListener` and new outbound
/// connections - those connections could sit in `.preparing`/`.waiting`
/// indefinitely, silently swallowing the send instead of just using a bad
/// port. A single real socket removes the whole class of problem: every
/// datagram - deliver or ack - always leaves from, and is always received on,
/// the exact same port, so there's no address to go stale in the first place.
/// All callbacks are pinned to the main queue so this file stays inside the
/// module's default `MainActor` isolation with no `nonisolated` escape hatches.
final class LanUdpLink: NSObject, PeerTransport {
    /// Bonjour service type. Matches the Rust side's
    /// `_claudepet._udp.local.` registration and the `NSBonjourServices` entry
    /// already in `Scripts/bundle.sh`'s Info.plist.
    private static let serviceType = "_claudepet._udp."
    /// How long a source address learned from an inbound datagram is trusted
    /// over a freshly Bonjour-resolved advertised address. Mirrors the Rust
    /// side's `LEARNED_TTL`.
    private static let learnedTTL: TimeInterval = 5 * 60

    /// A peer's known addresses. `learned` (the source address of its most
    /// recent datagram to us) is preferred while fresh - proven reachable
    /// right now - over `advertised` (from a Bonjour resolve, which can be a
    /// virtual adapter's address on a multi-homed host, or briefly regress a
    /// working address). Kept as two fields, never clobbering one on every
    /// resolve, so a resolve can never regress a working send/ack loop.
    private struct PeerAddrs {
        var advertised: sockaddr_storage?
        var learned: (sockaddr_storage, Date)?
        var best: sockaddr_storage? {
            if let (addr, at) = learned, Date().timeIntervalSince(at) < learnedTTL { return addr }
            return advertised
        }
    }

    private var socketFD: Int32 = -1
    private var readSource: DispatchSourceRead?
    private var localPort: UInt16 = 0

    private var netService: NetService?
    private var browser: NetServiceBrowser?
    /// Services mid-resolve must be kept alive (NetService drops its delegate
    /// callbacks if deallocated) until `netServiceDidResolveAddress` fires.
    private var resolvingServices: Set<NetService> = []

    private var peers: [String: PeerAddrs] = [:]

    var onPeersChanged: (([String]) -> Void)?
    var onReceive: ((PetMessage, String) -> Void)?

    /// Same identity the MultipeerConnectivity link uses, so a peer shows up
    /// under one name regardless of which transport reached it.
    private let localName: String

    override init() {
        localName = MultipeerLink.localDisplayName
        super.init()
    }

    /// Test-only: lets two instances run in the same process (which share
    /// `MultipeerLink.localDisplayName`) without colliding.
    init(overrideLocalName: String) {
        localName = overrideLocalName
        super.init()
    }

    var peerNames: [String] {
        peers.keys.filter { $0 != localName }.sorted()
    }

    func start() {
        openSocket()
        publishService()
        startBrowsing()
    }

    /// Tears down the socket/service/browser so this pet drops off the
    /// Bonjour browse promptly instead of lingering until the peer's cache
    /// expires - same rationale as `MultipeerLink.stop()`.
    func stop() {
        browser?.stop()
        browser = nil
        resolvingServices.removeAll()
        netService?.stop()
        netService = nil
        readSource?.cancel()
        readSource = nil
        if socketFD >= 0 {
            close(socketFD)
            socketFD = -1
        }
    }

    // MARK: - Socket

    private func openSocket() {
        let fd = socket(AF_INET6, SOCK_DGRAM, 0)
        guard fd >= 0 else {
            NSLog("ClaudePet LanUdpLink: socket() failed: \(String(cString: strerror(errno)))")
            return
        }
        var off: Int32 = 0
        setsockopt(fd, IPPROTO_IPV6, IPV6_V6ONLY, &off, socklen_t(MemoryLayout<Int32>.size))

        var addr = sockaddr_in6()
        addr.sin6_len = UInt8(MemoryLayout<sockaddr_in6>.size)
        addr.sin6_family = sa_family_t(AF_INET6)
        addr.sin6_port = 0
        addr.sin6_addr = in6addr_any
        let bindResult = withUnsafePointer(to: &addr) { ptr -> Int32 in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sa in
                Darwin.bind(fd, sa, socklen_t(MemoryLayout<sockaddr_in6>.size))
            }
        }
        guard bindResult == 0 else {
            NSLog("ClaudePet LanUdpLink: bind() failed: \(String(cString: strerror(errno)))")
            close(fd)
            return
        }

        var bound = sockaddr_in6()
        var boundLen = socklen_t(MemoryLayout<sockaddr_in6>.size)
        _ = withUnsafeMutablePointer(to: &bound) { ptr -> Int32 in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sa in
                getsockname(fd, sa, &boundLen)
            }
        }
        localPort = UInt16(bigEndian: bound.sin6_port)
        socketFD = fd

        let source = DispatchSource.makeReadSource(fileDescriptor: fd, queue: .main)
        source.setEventHandler { [weak self] in
            MainActor.assumeIsolated { self?.readAvailable() }
        }
        source.setCancelHandler { close(fd) }
        source.resume()
        readSource = source
    }

    private func readAvailable() {
        guard socketFD >= 0 else { return }
        var buf = [UInt8](repeating: 0, count: 64 * 1024)
        var from = sockaddr_storage()
        var fromLen = socklen_t(MemoryLayout<sockaddr_storage>.size)
        let n = withUnsafeMutablePointer(to: &from) { ptr -> Int in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sa in
                recvfrom(socketFD, &buf, buf.count, 0, sa, &fromLen)
            }
        }
        guard n > 0 else { return }
        let data = Data(buf[0..<n])
        guard let wire = try? JSONDecoder().decode(LanWireMessage.self, from: data),
              let message = wire.toPetMessage()
        else { return }

        // Inbound datagrams are proof the sender is alive and reachable
        // *right now* - remember the source address so replies (acks,
        // follow-ups) go straight to a working socket even if Bonjour
        // resolution never completed, or resolved to a different address than
        // the one traffic actually flows on. Never record our own name - a
        // loopback quirk during discovery must not show up as a peer.
        if message.senderName != localName {
            var entry = peers[message.senderName] ?? PeerAddrs()
            entry.learned = (from, Date())
            peers[message.senderName] = entry
            onPeersChanged?(peerNames)
        }
        onReceive?(message, message.senderName)
    }

    // MARK: - Discovery

    private func publishService() {
        guard localPort != 0 else { return }
        let service = NetService(domain: "", type: Self.serviceType, name: localName, port: Int32(localPort))
        service.delegate = self
        service.publish()
        netService = service
    }

    private func startBrowsing() {
        let browser = NetServiceBrowser()
        browser.delegate = self
        browser.searchForServices(ofType: Self.serviceType, inDomain: "")
        self.browser = browser
    }

    private func restartBrowserForRetry() {
        browser?.stop()
        browser = nil
        startBrowsing()
    }

    // MARK: - Outbound

    func send(_ message: PetMessage, to peerName: String) {
        guard socketFD >= 0 else { return }
        guard let addr = peers[peerName]?.best else {
            // Unknown peer: the passive browse can lag reality (peer just
            // relaunched, browser mid-restart). Restart discovery once and
            // retry after a beat before giving up - silently dropping here
            // was the old behavior and produced messages that vanished with
            // no bubble at all.
            NSLog("ClaudePet LanUdpLink: send to unknown peer \(peerName) - restarting browse")
            restartBrowserForRetry()
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.6) { [weak self] in
                guard let self, self.peers[peerName]?.best != nil else {
                    NSLog("ClaudePet LanUdpLink: peer \(peerName) still unknown after browse restart")
                    return
                }
                self.send(message, to: peerName)
            }
            return
        }
        guard let data = try? JSONEncoder().encode(LanWireMessage(message)) else { return }

        var target = addr
        let len = socklen_t(sockLen(of: target))
        let result = withUnsafePointer(to: &target) { ptr -> Int in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sa in
                data.withUnsafeBytes { buf in
                    sendto(socketFD, buf.baseAddress, buf.count, 0, sa, len)
                }
            }
        }
        if result < 0 {
            NSLog("ClaudePet LanUdpLink: sendto \(peerName) failed: \(String(cString: strerror(errno)))")
        }
    }

    private func sockLen(of addr: sockaddr_storage) -> Int {
        switch Int32(addr.ss_family) {
        case AF_INET: return MemoryLayout<sockaddr_in>.size
        case AF_INET6: return MemoryLayout<sockaddr_in6>.size
        default: return MemoryLayout<sockaddr_storage>.size
        }
    }
}

extension LanUdpLink: NetServiceBrowserDelegate {
    func netServiceBrowser(_ browser: NetServiceBrowser, didFind service: NetService, moreComing: Bool) {
        guard service.name != localName else { return }
        service.delegate = self
        resolvingServices.insert(service)
        service.resolve(withTimeout: 5)
    }

    func netServiceBrowser(_ browser: NetServiceBrowser, didRemove service: NetService, moreComing: Bool) {
        guard service.name != localName else { return }
        peers.removeValue(forKey: service.name)
        onPeersChanged?(peerNames)
    }

    func netServiceBrowser(_ browser: NetServiceBrowser, didNotSearch errorDict: [String: NSNumber]) {
        // A browser can die silently - wake-from-sleep, an interface change, a
        // transient failure - which freezes the peer list at whatever it last
        // saw. Re-create it so passive discovery resumes.
        guard self.browser === browser else { return }
        self.browser = nil
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) { [weak self] in
            self?.startBrowsing()
        }
    }
}

extension LanUdpLink: NetServiceDelegate {
    func netServiceDidResolveAddress(_ sender: NetService) {
        resolvingServices.remove(sender)
        guard let addr = Self.pickAddress(from: sender.addresses ?? []) else { return }
        var entry = peers[sender.name] ?? PeerAddrs()
        entry.advertised = LanUdpLink.mappedToV6(addr)
        peers[sender.name] = entry
        onPeersChanged?(peerNames)
    }

    /// The socket is opened `AF_INET6` (dual-stack via `IPV6_V6ONLY` off under
    /// the hood, but the syscall-level `sendto` still requires an `AF_INET6`
    /// destination) - map a resolved IPv4 peer address to its IPv4-mapped IPv6
    /// form rather than handing `sendto` an `AF_INET` sockaddr, which fails
    /// with EINVAL. `recvfrom` already returns IPv4 senders pre-mapped this
    /// way, so only Bonjour-resolved (`advertised`) addresses need this.
    /// Mirrors the Rust side's `SocketAddr::V4 -> V6` conversion in `send()`.
    private static func mappedToV6(_ addr: sockaddr_storage) -> sockaddr_storage {
        guard Int32(addr.ss_family) == AF_INET else { return addr }
        var v4 = sockaddr_storage_to(addr, as: sockaddr_in.self)
        var mapped = sockaddr_in6()
        mapped.sin6_len = UInt8(MemoryLayout<sockaddr_in6>.size)
        mapped.sin6_family = sa_family_t(AF_INET6)
        mapped.sin6_port = v4.sin_port
        withUnsafeMutableBytes(of: &mapped.sin6_addr) { dst in
            withUnsafeBytes(of: &v4.sin_addr) { src in
                // ::ffff:a.b.c.d - the last 4 bytes are the IPv4 address, the
                // 2 bytes before that are 0xffff, everything before is zero.
                dst[10] = 0xff
                dst[11] = 0xff
                dst[12] = src[0]
                dst[13] = src[1]
                dst[14] = src[2]
                dst[15] = src[3]
            }
        }
        var storage = sockaddr_storage()
        withUnsafeMutableBytes(of: &storage) { dst in
            withUnsafeBytes(of: &mapped) { src in
                dst.copyMemory(from: UnsafeRawBufferPointer(rebasing: src[0..<src.count]))
            }
        }
        return storage
    }

    func netService(_ sender: NetService, didNotResolve errorDict: [String: NSNumber]) {
        resolvingServices.remove(sender)
    }

    /// Pick the best reachable address for a resolved peer. IPv4 first, then a
    /// non-link-local IPv6, then a scoped link-local IPv6 (the sockaddr blobs
    /// `NetService.addresses` returns carry the interface scope id, unlike
    /// `NWBrowser`, so a link-local v6 is usable here). Mirrors the Rust
    /// side's `pick_peer_addr`.
    fileprivate static func pickAddress(from datas: [Data]) -> sockaddr_storage? {
        var v6Fallback: sockaddr_storage?
        var v6LinkLocal: sockaddr_storage?
        for data in datas {
            var storage = sockaddr_storage()
            _ = withUnsafeMutableBytes(of: &storage) { dst in
                data.copyBytes(to: dst, count: min(data.count, dst.count))
            }
            switch Int32(storage.ss_family) {
            case AF_INET:
                return storage
            case AF_INET6:
                let in6 = withUnsafePointer(to: &storage) { ptr -> sockaddr_in6 in
                    ptr.withMemoryRebound(to: sockaddr_in6.self, capacity: 1) { $0.pointee }
                }
                if IN6_IS_ADDR_LINKLOCAL(in6.sin6_addr) {
                    if v6LinkLocal == nil { v6LinkLocal = storage }
                } else if v6Fallback == nil {
                    v6Fallback = storage
                }
            default:
                break
            }
        }
        return v6Fallback ?? v6LinkLocal
    }
}

/// `in6_addr` has no public link-local test on Darwin's Swift overlay.
private func IN6_IS_ADDR_LINKLOCAL(_ addr: in6_addr) -> Bool {
    var a = addr
    return withUnsafeBytes(of: &a) { $0[0] == 0xfe && ($0[1] & 0xc0) == 0x80 }
}

private func sockaddr_storage_to<T>(_ storage: sockaddr_storage, as type: T.Type) -> T {
    var s = storage
    return withUnsafePointer(to: &s) { ptr in
        ptr.withMemoryRebound(to: T.self, capacity: 1) { $0.pointee }
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
