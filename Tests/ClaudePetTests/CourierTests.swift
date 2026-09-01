import Testing
@testable import ClaudePet
import Foundation

struct CourierTests {

    @Test func outboundDepartsThenWaitsAtTheEdge() {
        let start = Date(timeIntervalSince1970: 0)
        let courier = Courier.outbound(startX: 100, homeX: 100, offScreenX: 300, edge: .right, now: start)
        #expect(courier.phase == .departing)
        #expect(courier.facingRight == true)

        // Enough real time to cover the 200pt gap at 90pt/s.
        courier.tick(now: start.addingTimeInterval(3))
        #expect(courier.phase == .away)
        #expect(abs(courier.x - 300) < 0.01)
    }

    @Test func outboundReturnsHomeOnAck() {
        let start = Date(timeIntervalSince1970: 0)
        let courier = Courier.outbound(startX: 100, homeX: 100, offScreenX: 300, edge: .right, now: start)
        courier.tick(now: start.addingTimeInterval(3)) // now .away
        #expect(courier.phase == .away)

        courier.receivedAck()
        #expect(courier.phase == .returning)

        courier.tick(now: start.addingTimeInterval(6))
        #expect(courier.phase == .done)
        #expect(abs(courier.x - 100) < 0.01)
    }

    @Test func outboundTimesOutAndReturnsWithoutAck() {
        let start = Date(timeIntervalSince1970: 0)
        let courier = Courier.outbound(startX: 100, homeX: 100, offScreenX: 300, edge: .right, now: start)
        courier.tick(now: start.addingTimeInterval(3)) // now .away
        #expect(courier.phase == .away)

        // Not yet at the timeout.
        courier.tick(now: start.addingTimeInterval(5))
        #expect(courier.phase == .away)

        // Past the 10s timeout, with no ack ever received.
        courier.tick(now: start.addingTimeInterval(11))
        #expect(courier.phase == .returning)
    }

    @Test func receivedAckIsIgnoredOutsideAwayPhase() {
        let start = Date(timeIntervalSince1970: 0)
        let courier = Courier.outbound(startX: 100, homeX: 100, offScreenX: 300, edge: .right, now: start)
        #expect(courier.phase == .departing)
        courier.receivedAck() // no-op while still departing
        #expect(courier.phase == .departing)
    }

    @Test func inboundArrivesHandsOffThenLeaves() {
        let start = Date(timeIntervalSince1970: 0)
        let courier = Courier.inbound(offScreenX: -200, handoffX: 40, edge: .left, now: start)
        #expect(courier.phase == .arriving)
        #expect(courier.facingRight == true) // walking rightward, toward the resident pet

        courier.tick(now: start.addingTimeInterval(3))
        #expect(courier.phase == .handing)
        #expect(abs(courier.x - 40) < 0.01)

        // Handoff duration is 2.2s - not done until then.
        courier.tick(now: start.addingTimeInterval(4))
        #expect(courier.phase == .leaving)

        courier.tick(now: start.addingTimeInterval(10))
        #expect(courier.phase == .done)
        #expect(abs(courier.x - (-200)) < 0.01)
    }

    @Test func animReflectsMovementVsPause() {
        let start = Date(timeIntervalSince1970: 0)
        let courier = Courier.outbound(startX: 100, homeX: 100, offScreenX: 300, edge: .right, now: start)
        #expect(courier.anim == .walk)
        courier.tick(now: start.addingTimeInterval(3))
        #expect(courier.phase == .away)
        #expect(courier.anim == .idle)
    }
}
