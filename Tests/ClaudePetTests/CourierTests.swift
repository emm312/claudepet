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

    @Test func outboundReturnsHomeAfterTheRecipientsWaitElapses() {
        let start = Date(timeIntervalSince1970: 0)
        let courier = Courier.outbound(startX: 100, homeX: 100, offScreenX: 300, edge: .right, now: start)
        courier.tick(now: start.addingTimeInterval(3)) // now .away (entered at start+3)
        #expect(courier.phase == .away)

        // Ack arrives instantly but says the recipient's own visitor needs 2s.
        courier.receivedAck(wait: 2, now: start.addingTimeInterval(3.01))
        // Too soon - the horse shouldn't beat the recipient's own animation.
        #expect(courier.phase == .away)

        courier.tick(now: start.addingTimeInterval(4.5)) // still short of start+3+2
        #expect(courier.phase == .away)

        courier.tick(now: start.addingTimeInterval(5.01)) // now past start+3+2 - the pending ack applies
        #expect(courier.phase == .returning)

        courier.tick(now: start.addingTimeInterval(8.01))
        #expect(courier.phase == .done)
        #expect(abs(courier.x - 100) < 0.01)
    }

    @Test func ackThatArrivesAfterTheWaitAlreadyElapsedReturnsImmediately() {
        let start = Date(timeIntervalSince1970: 0)
        let courier = Courier.outbound(startX: 100, homeX: 100, offScreenX: 300, edge: .right, now: start)
        courier.tick(now: start.addingTimeInterval(3))
        #expect(courier.phase == .away)

        // The recipient's own animation only needed 1s, and this ack shows up
        // well after that - nothing left to wait for.
        courier.receivedAck(wait: 1, now: start.addingTimeInterval(6))
        #expect(courier.phase == .returning)
    }

    @Test func outboundTimesOutAndReturnsWithoutAck() {
        let start = Date(timeIntervalSince1970: 0)
        let courier = Courier.outbound(startX: 100, homeX: 100, offScreenX: 300, edge: .right, now: start)
        courier.tick(now: start.addingTimeInterval(3)) // now .away
        #expect(courier.phase == .away)

        // Not yet at the timeout (deadline is +3s + 15s away-timeout = +18s).
        courier.tick(now: start.addingTimeInterval(14))
        #expect(courier.phase == .away)

        // Past the 15s timeout, with no ack ever received.
        courier.tick(now: start.addingTimeInterval(19))
        #expect(courier.phase == .returning)
    }

    @Test func receivedAckIsIgnoredOutsideAwayPhase() {
        let start = Date(timeIntervalSince1970: 0)
        let courier = Courier.outbound(startX: 100, homeX: 100, offScreenX: 300, edge: .right, now: start)
        #expect(courier.phase == .departing)
        courier.receivedAck(wait: 2, now: start) // doesn't change phase while still departing
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

        // Short "touch and go" handoff (0.5s): still handing a moment in...
        courier.tick(now: start.addingTimeInterval(3.2))
        #expect(courier.phase == .handing)

        // ...and gone once it elapses.
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
