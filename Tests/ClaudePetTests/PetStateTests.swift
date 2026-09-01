import Testing
@testable import ClaudePet
import Foundation

struct PetStateTests {

    @Test func decayOverOneHour() {
        var state = PetState()
        let start = Date(timeIntervalSince1970: 0)
        state.lastTick = start
        state.tick(now: start.addingTimeInterval(3600))

        #expect(abs(state.hunger - 77) < 0.01)
        #expect(abs(state.energy - 98) < 0.01)
        #expect(abs(state.happiness - 78.5) < 0.01)
        #expect(abs(state.cleanliness - 99) < 0.01)
    }

    @Test func statsNeverGoBelowZero() {
        var state = PetState()
        let start = Date(timeIntervalSince1970: 0)
        state.lastTick = start
        // 100 days is enough to bottom out every stat given the tuned decay rates.
        state.tick(now: start.addingTimeInterval(100 * 86400))
        #expect(state.hunger >= 0)
        #expect(state.energy >= 0)
        #expect(state.happiness >= 0)
        #expect(state.cleanliness >= 0)
    }

    @Test func statsNeverExceedOneHundred() {
        var state = PetState()
        state.feed(); state.feed(); state.feed(); state.feed(); state.feed()
        state.play(); state.play(); state.play()
        state.pet(); state.pet(); state.pet()
        #expect(state.hunger <= 100)
        #expect(state.happiness <= 100)
    }

    @Test func longAbsenceIsClampedNotZeroed() {
        let start = Date(timeIntervalSince1970: 0)

        // Both should be clamped at 0 given a long enough real gap, but the
        // mechanism under test is that the elapsed time used internally is
        // capped - verify via a case just under the cap where hunger is not
        // yet zero, and a case well over the cap that matches it.
        var underCap = PetState()
        underCap.lastTick = start
        underCap.tick(now: start.addingTimeInterval(11 * 3600)) // under 12h cap
        #expect(underCap.hunger > 0)

        var overCap = PetState()
        overCap.lastTick = start
        overCap.tick(now: start.addingTimeInterval(50 * 3600)) // well over 12h cap
        // Should match the 12h-capped result, not a further-decayed one.
        #expect(abs(overCap.hunger - underCap.hunger) < 5.0)
    }

    @Test func negativeClockDeltaIsIgnored() {
        var state = PetState()
        let start = Date(timeIntervalSince1970: 10_000)
        state.lastTick = start
        let before = state.hunger
        state.tick(now: start.addingTimeInterval(-3600)) // clock went backwards
        #expect(abs(state.hunger - before) < 0.001)
    }

    @Test func moodPriorityHungerOverTiredness() {
        var state = PetState()
        state.hunger = 10
        state.energy = 10
        #expect(state.mood == .hungry)
    }
}
