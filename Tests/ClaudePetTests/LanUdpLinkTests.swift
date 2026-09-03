import Testing
@testable import ClaudePet
import Foundation

/// Proves two `LanUdpLink` instances actually discover each other over
/// Bonjour and round-trip a deliver + ack over the real UDP socket path -
/// the same shape of bug (acks silently lost) survived two earlier attempts
/// at this file that only looked right on paper, so this is exercised for
/// real rather than trusted by inspection.
@MainActor
struct LanUdpLinkTests {
    @Test func discoversPeerAndRoundTripsDeliverAndAck() async throws {
        let nameA = "TestLinkA-\(UUID().uuidString.prefix(8))"
        let nameB = "TestLinkB-\(UUID().uuidString.prefix(8))"

        let a = LanUdpLink(overrideLocalName: nameA)
        let b = LanUdpLink(overrideLocalName: nameB)

        var bReceived: (PetMessage, String)?
        b.onReceive = { message, peer in bReceived = (message, peer) }
        var aReceivedAck: (PetMessage, String)?
        a.onReceive = { message, peer in aReceivedAck = (message, peer) }

        a.start()
        b.start()
        defer { a.stop(); b.stop() }

        // Wait for mutual Bonjour discovery (both directions - each side
        // needs the other's address before it can send).
        let discovered = try await waitUntil(timeout: 8) {
            a.peerNames.contains(nameB) && b.peerNames.contains(nameA)
        }
        #expect(discovered, "A and B should discover each other via Bonjour")

        let sent = PetMessage.deliver(text: "ping", senderName: nameA, exitEdge: .right)
        a.send(sent, to: nameB)

        let delivered = try await waitUntil(timeout: 5) { bReceived != nil }
        #expect(delivered, "B should receive A's delivery")
        #expect(bReceived?.0.text == "ping")
        #expect(bReceived?.0.id == sent.id)
        #expect(bReceived?.1 == nameA)

        // B acks back - this is the path that was broken: an ack sent from a
        // fresh ephemeral port (or a hung requiredLocalEndpoint connection)
        // never reached A.
        b.send(sent.makeAck(from: nameB, timeToReturn: 1.5), to: nameA)

        let acked = try await waitUntil(timeout: 5) { aReceivedAck != nil }
        #expect(acked, "A should receive B's ack")
        #expect(aReceivedAck?.0.kind == .ack)
        #expect(aReceivedAck?.0.id == sent.id)
        #expect(aReceivedAck?.1 == nameB, "the ack must be attributed to the acker, not re-echo the original sender")
    }
}

/// Polls `condition` on the main run loop until it's true or `timeout` elapses.
@MainActor
private func waitUntil(timeout: TimeInterval, _ condition: () -> Bool) async throws -> Bool {
    let deadline = Date().addingTimeInterval(timeout)
    while Date() < deadline {
        if condition() { return true }
        try await Task.sleep(nanoseconds: 100_000_000)
    }
    return condition()
}
