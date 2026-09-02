import Testing
@testable import ClaudePet
import Foundation

struct RampageTests {
    /// Deterministic RNG cycling through a fixed sequence, so target picks
    /// are reproducible across runs.
    private static func seededRNG(_ values: [Double]) -> () -> Double {
        var index = 0
        return {
            let value = values[index % values.count]
            index += 1
            return value
        }
    }

    private static let frame = CGRect(x: 100, y: 100, width: 400, height: 300)
    private static let petSize = CGSize(width: 80, height: 80)

    @Test func targetsStayInsideTheFrame() {
        let start = Date(timeIntervalSince1970: 0)
        let rampage = Rampage(frame: Self.frame, petSize: Self.petSize, currentPosition: Self.frame.origin, now: start, rng: Self.seededRNG([0, 1, 0.5, 0.25, 0.75]))

        var position = CGPoint.zero
        for i in 0..<200 {
            position = rampage.tick(now: start.addingTimeInterval(Double(i) * 0.05), dt: 0.05)
            #expect(position.x >= Self.frame.minX - 0.01)
            #expect(position.x <= Self.frame.maxX - Self.petSize.width + 0.01)
            #expect(position.y >= Self.frame.minY - 0.01)
            #expect(position.y <= Self.frame.maxY - Self.petSize.height + 0.01)
        }
    }

    @Test func facingRightFollowsMovementDirection() {
        let start = Date(timeIntervalSince1970: 0)
        // First target far to the right of the initial (frame.origin) position.
        let rampage = Rampage(frame: Self.frame, petSize: Self.petSize, currentPosition: Self.frame.origin, now: start, rng: Self.seededRNG([1.0, 1.0]))
        _ = rampage.tick(now: start.addingTimeInterval(0.05), dt: 0.05)
        #expect(rampage.facingRight == true)
    }

    @Test func tiersAdvanceAtThirtyAndNinetySeconds() {
        let start = Date(timeIntervalSince1970: 0)
        let rampage = Rampage(frame: Self.frame, petSize: Self.petSize, currentPosition: Self.frame.origin, now: start, rng: Self.seededRNG([0.5]))

        _ = rampage.tick(now: start.addingTimeInterval(10), dt: 0.05)
        #expect(rampage.tier == .warmup)

        _ = rampage.tick(now: start.addingTimeInterval(31), dt: 0.05)
        #expect(rampage.tier == .nagging)

        _ = rampage.tick(now: start.addingTimeInterval(91), dt: 0.05)
        #expect(rampage.tier == .furious)
    }

    @Test func speedIncreasesPerTier() {
        let start = Date(timeIntervalSince1970: 0)
        // Always retarget to the far corner so distance never runs out.
        func distanceCovered(elapsed: TimeInterval) -> CGFloat {
            let rampage = Rampage(frame: Self.frame, petSize: Self.petSize, currentPosition: Self.frame.origin, now: start, rng: Self.seededRNG([1.0]))
            let before = rampage.tick(now: start.addingTimeInterval(elapsed), dt: 0.0001)
            let after = rampage.tick(now: start.addingTimeInterval(elapsed + 0.1), dt: 0.1)
            let deltaX = after.x - before.x
            let deltaY = after.y - before.y
            return (deltaX * deltaX + deltaY * deltaY).squareRoot()
        }

        let warmupDistance = distanceCovered(elapsed: 5)
        let naggingDistance = distanceCovered(elapsed: 35)
        let furiousDistance = distanceCovered(elapsed: 95)

        #expect(naggingDistance > warmupDistance)
        #expect(furiousDistance > naggingDistance)
    }

    @Test func updateTargetReclampsAnOutOfBoundsPosition() {
        let start = Date(timeIntervalSince1970: 0)
        let rampage = Rampage(frame: Self.frame, petSize: Self.petSize, currentPosition: Self.frame.origin, now: start, rng: Self.seededRNG([0.5]))
        _ = rampage.tick(now: start.addingTimeInterval(0.05), dt: 0.05)

        let smallerFrame = CGRect(x: 500, y: 500, width: 90, height: 90)
        rampage.updateTarget(frame: smallerFrame)
        let position = rampage.tick(now: start.addingTimeInterval(0.1), dt: 0.05)

        #expect(position.x >= smallerFrame.minX - 0.01)
        #expect(position.x <= smallerFrame.maxX - Self.petSize.width + 0.01)
        #expect(position.y >= smallerFrame.minY - 0.01)
        #expect(position.y <= smallerFrame.maxY - Self.petSize.height + 0.01)
    }

    @Test func shouldSpeakNowFiresOnceImmediatelyThenRespectsTierCadence() {
        let start = Date(timeIntervalSince1970: 0)
        let rampage = Rampage(frame: Self.frame, petSize: Self.petSize, currentPosition: Self.frame.origin, now: start, rng: Self.seededRNG([0.5]))

        #expect(rampage.shouldSpeakNow(now: start) == true) // always speaks immediately
        // Still warmup tier - no second bubble until nagging kicks in.
        #expect(rampage.shouldSpeakNow(now: start.addingTimeInterval(5)) == false)

        _ = rampage.tick(now: start.addingTimeInterval(31), dt: 0.05) // advance to .nagging
        // >10s has already elapsed since the very first bubble at t=0.
        #expect(rampage.shouldSpeakNow(now: start.addingTimeInterval(31)) == true)
        // Too soon after that one.
        #expect(rampage.shouldSpeakNow(now: start.addingTimeInterval(35)) == false)
        // ~10s after the t=31 bubble.
        #expect(rampage.shouldSpeakNow(now: start.addingTimeInterval(42)) == true)
    }
}
