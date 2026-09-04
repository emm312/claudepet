import Foundation

/// Canned speech-bubble lines, pooled by mood, with no-immediate-repeat.
/// All lines are deliberately buzzword-poisoned corporate jargon - the pet
/// talks like it just got back from an offsite.
enum Dialogue {
    private static let lines: [Mood: [String]] = [
        .happy: ["crushing it synergistically", "hitting my KPIs today", "living my best-practice life", "10x-ing the vibes"],
        .content: ["circling back to baseline", "low-key leveraging synergy", "*aligns on next steps*", "steady-state ideation"],
        .hungry: ["requesting more runway", "my hunger KPI is trending down", "need to fuel the growth engine", "feed me actionable nutrients"],
        .tired: ["running on fumes, not scalable", "recharging my synergy battery", "taking a strategic power nap", "zzz... optimizing offline"],
        .sad: ["my morale metrics are down", "feeling a bit off-roadmap", "need a win to move the needle", "*disengaged stakeholder energy*"],
        .dirty: ["requesting a full system refresh", "streamlining my hygiene stack", "ew, technical debt on my fur", "let's circle back on cleanliness"],
    ]

    // MARK: - Feed-triggered eating

    private static let celebrationLines = [
        "leaning into my north star!",
        "let's boil the ocean together!",
        "skinning the donkey of doubt!",
        "playing poker with poker - high risk, high reward!",
        "growth mindset: fully activated",
        "synergy unlocked, scaling this energy!",
        "circling back... to VICTORY",
        "10x growth mindset, let's go!",
        "moving the needle, one nutrient at a time",
        "this is a real value-add moment for me",
    ]

    private static var lastCelebrationLine: String?

    static func celebrationLine() -> String {
        var candidate = celebrationLines.randomElement() ?? "growth mindset: fully activated"
        var attempts = 0
        while candidate == lastCelebrationLine && attempts < 5 {
            candidate = celebrationLines.randomElement() ?? candidate
            attempts += 1
        }
        lastCelebrationLine = candidate
        return candidate
    }

    private static var lastLine: String?

    static func line(for mood: Mood) -> String {
        let pool = lines[mood] ?? ["..."]
        var candidate = pool.randomElement() ?? "..."
        if pool.count > 1 {
            var attempts = 0
            while candidate == lastLine && attempts < 5 {
                candidate = pool.randomElement() ?? candidate
                attempts += 1
            }
        }
        lastLine = candidate
        return candidate
    }

    // MARK: - Reels rage

    /// The very first thing the pet says on spotting Reels - always this line,
    /// regardless of tier, per the user's explicit ask.
    private static let warmupLine = "stop wasting ur bandwidth"

    private static let naggingLines = [
        "this is not a value-add activity",
        "let's align on priorities, not Reels",
        "that's outside our core competency",
        "please re-focus on the North Star",
        "boil the ocean later, ship now",
        "stop skinning the donkey on side quests",
        "let's not play poker with poker, focus up",
        "circle back to your OKRs",
        "I will not be de-prioritized",
        "put the phone down, it's not on the roadmap",
        "stop wasting ur bandwidth, seriously",
        "this bandwidth could be better allocated",
    ]

    private static let furiousLines = [
        "STOP WASTING UR BANDWIDTH",
        "THIS IS NOT A VALUE-ADD ACTIVITY",
        "PUT THE PHONE DOWN",
        "I WILL NOT BE DE-PRIORITIZED",
        "CIRCLE BACK TO YOUR OKRS RIGHT NOW",
    ]

    private static var lastAngryLine: String?

    static func angryLine(tier: Rampage.Tier) -> String {
        if tier == .warmup { return warmupLine }
        let pool = tier == .furious ? furiousLines : naggingLines
        var candidate = pool.randomElement() ?? warmupLine
        var attempts = 0
        while candidate == lastAngryLine && attempts < 5 {
            candidate = pool.randomElement() ?? candidate
            attempts += 1
        }
        lastAngryLine = candidate
        return candidate
    }

    // MARK: - Pet-to-pet messaging

    private static let departLines = [
        "off to sync up cross-functionally",
        "taking this offline",
        "let's take this conversation async",
        "going to close the loop in person",
    ]

    static func departLine() -> String {
        departLines.randomElement() ?? "taking this offline"
    }

    private static let deliveryFailedLines = [
        "couldn't find them, going back to my desk",
        "no signal on that stakeholder, retrying later",
        "message bounced, circling back",
    ]

    static func deliveryFailedLine() -> String {
        deliveryFailedLines.randomElement() ?? "couldn't find them, going back to my desk"
    }
}
