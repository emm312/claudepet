import Foundation
import CoreGraphics

/// Drives the pet's position while it's rampaging across a Reels browser
/// window: chaotic darting between random points inside the window's frame,
/// escalating in speed and nagging frequency the longer it goes on.
///
/// Pure logic (no AppKit windows), driven by an injected `now` and an
/// injected RNG - unit-testable exactly like `Brain` and `Courier`. The
/// Runtime owns one of these while a `DistractionSighting` frame is present,
/// feeding it fresh frames as the browser window moves/resizes, and reads
/// `tick(now:dt:)` each frame instead of running gravity/`Brain` at all.
final class Rampage {
    enum Tier: Equatable {
        case warmup   // 0-30s: one bubble, then quiet
        case nagging  // 30-90s: a bubble every ~10s
        case furious  // 90s+: a bubble every ~3.5s, fastest movement
    }

    private static let warmupDuration: TimeInterval = 30
    private static let naggingDuration: TimeInterval = 90

    // Fast from the very first frame - this is meant to read as chaotic and
    // hard to miss immediately, not ramp up gently. Escalation past warmup
    // still makes it faster still.
    private static let speedByTier: [Tier: CGFloat] = [
        .warmup: 260,
        .nagging: 380,
        .furious: 520,
    ]
    /// `.infinity` means "only the first bubble, then stay quiet" - handled
    /// specially in `shouldSpeakNow`, since a real bubble interval owns the
    /// re-triggering after that.
    private static let bubbleIntervalByTier: [Tier: TimeInterval] = [
        .warmup: .infinity,
        .nagging: 10,
        .furious: 3.5,
    ]

    private static let arrivalEpsilon: CGFloat = 4
    // Barely a pause between darts - at these speeds a longer dwell reads as
    // the pet stopping to think, which undercuts the "chaotic scurry" ask.
    private static let dwellDuration: TimeInterval = 0.04

    private let startDate: Date
    private var frame: CGRect
    private let petSize: CGSize
    private let rng: () -> Double

    private var position: CGPoint
    private var target: CGPoint
    private var dwellUntil: Date = .distantPast
    private(set) var facingRight = true
    private var lastBubbleDate: Date

    /// Escalation tier as of the last `tick`. Computed fresh each tick from
    /// `startDate`, so it's always consistent with the current time.
    private(set) var tier: Tier = .warmup

    /// - Parameters:
    ///   - frame: the browser window's frame, in AppKit bottom-left-origin
    ///     coordinates - the rectangle to scurry inside.
    ///   - petSize: the pet's own window size, used to keep it fully inside
    ///     `frame` rather than letting it hang off an edge.
    ///   - currentPosition: the pet's actual on-screen origin the instant
    ///     distraction was detected. Used as-is, deliberately **not** clamped
    ///     into `frame` - the pet is very likely standing somewhere outside
    ///     the browser window when this fires (on a ledge elsewhere on
    ///     screen), and starting it at the *browser's* corner instead of its
    ///     own current spot would snap it there instantly. Leaving it
    ///     unclamped means the first `tick()` just walks it in from wherever
    ///     it already is, toward the first randomly-picked target inside
    ///     `frame`, the same way any other movement here works.
    init(
        frame: CGRect, petSize: CGSize, currentPosition: CGPoint,
        now: Date = Date(), rng: @escaping () -> Double = { Double.random(in: 0...1) }
    ) {
        self.startDate = now
        self.frame = frame
        self.petSize = petSize
        self.rng = rng
        self.position = currentPosition
        self.target = currentPosition
        // Distant past so the very first tick always speaks immediately.
        self.lastBubbleDate = .distantPast
        pickNewTarget()
    }

    /// Call when the sighted browser window moves or resizes; re-clamps the
    /// current position and picks a fresh target inside the new bounds.
    func updateTarget(frame: CGRect) {
        self.frame = frame
        position = Self.clampedOrigin(position, frame: frame, petSize: petSize)
        pickNewTarget()
    }

    /// Advances the darting motion by `dt` and returns the next window
    /// origin. Also updates `tier` and decides (via `shouldSpeakNow`,
    /// checked by the caller) when to speak.
    func tick(now: Date, dt: TimeInterval) -> CGPoint {
        tier = Self.tier(elapsed: now.timeIntervalSince(startDate))

        if now < dwellUntil {
            return position
        }

        let speed = Self.speedByTier[tier] ?? Self.speedByTier[.warmup]!
        let dx = target.x - position.x
        let dy = target.y - position.y
        let distance = (dx * dx + dy * dy).squareRoot()

        if distance <= Self.arrivalEpsilon {
            dwellUntil = now.addingTimeInterval(Self.dwellDuration)
            pickNewTarget()
            return position
        }

        let step = CGFloat(dt) * speed
        if step >= distance {
            position = target
        } else {
            position.x += dx / distance * step
            position.y += dy / distance * step
        }
        if dx != 0 { facingRight = dx > 0 }
        return position
    }

    /// True if a nag bubble should fire this tick, given the current tier -
    /// call once per `tick(now:dt:)` and show a bubble if this returns true.
    /// Not folded into `tick` itself so the caller controls bubble text/UI.
    func shouldSpeakNow(now: Date) -> Bool {
        if lastBubbleDate == .distantPast {
            lastBubbleDate = now
            return true
        }
        guard tier != .warmup else { return false } // one-and-done until nagging kicks in
        let interval = Self.bubbleIntervalByTier[tier] ?? .infinity
        guard now.timeIntervalSince(lastBubbleDate) >= interval else { return false }
        lastBubbleDate = now
        return true
    }

    private func pickNewTarget() {
        let minX = frame.minX
        let maxX = max(minX, frame.maxX - petSize.width)
        let minY = frame.minY
        let maxY = max(minY, frame.maxY - petSize.height)
        target = CGPoint(
            x: minX + CGFloat(rng()) * (maxX - minX),
            y: minY + CGFloat(rng()) * (maxY - minY)
        )
    }

    private static func clampedOrigin(_ origin: CGPoint, frame: CGRect, petSize: CGSize) -> CGPoint {
        let maxX = max(frame.minX, frame.maxX - petSize.width)
        let maxY = max(frame.minY, frame.maxY - petSize.height)
        return CGPoint(
            x: min(max(origin.x, frame.minX), maxX),
            y: min(max(origin.y, frame.minY), maxY)
        )
    }

    private static func tier(elapsed: TimeInterval) -> Tier {
        if elapsed < warmupDuration { return .warmup }
        if elapsed < naggingDuration { return .nagging }
        return .furious
    }
}
