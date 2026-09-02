import Foundation
import CoreGraphics

/// Animation/behavior states - separate from `Mood` (which is about stats). A
/// `happy` pet can still be in `.walk` or `.idle` anim state.
enum PetMood {
    enum AnimState: String {
        case idle, walk, sleep, sad, dragged, angry, fall, dance
    }
}

/// Drives what the pet is currently doing on screen: idle/walk/sit/sleep/etc,
/// biased toward stillness so it reads as alive without being twitchy or
/// annoying to have on screen all day.
///
/// The one deliberate exception is `isDistracted`: while the user is watching
/// Reels, the Brain only picks the `.angry` sprite - actual movement (darting
/// around the browser window) is owned by `Rampage`, driven separately by the
/// Runtime, since the Brain has no notion of where that window is.
final class Brain {
    private(set) var anim: PetMood.AnimState = .idle
    private(set) var facingRight = true
    private var stateEndTime: Date = .distantPast
    private var isBeingDragged = false
    private(set) var isDistracted = false
    private(set) var isFalling = false
    private(set) var isDancing = false

    private let rng: () -> Double

    private static let walkSpeed: CGFloat = 1.2
    /// Fallback sprint speed for when the pet is distracted but no `Rampage`
    /// is driving it (the Accessibility read matched the URL but couldn't get
    /// window geometry) - see `Runtime.applySighting`.
    private static let angrySpeed: CGFloat = 4.5
    private static let danceDuration: TimeInterval = 2.6
    private static let danceShimmyAmplitude: CGFloat = 6
    private static let danceShimmyRate: Double = 5.5 // radians/sec

    private var danceStartTime: Date = .distantPast
    private var lastShimmyOffset: CGFloat = 0

    init(rng: @escaping () -> Double = { Double.random(in: 0...1) }) {
        self.rng = rng
    }

    func beginDrag() {
        isBeingDragged = true
        isDancing = false
        anim = .dragged
    }

    func endDrag() {
        isBeingDragged = false
        stateEndTime = .distantPast // force a re-pick next tick
    }

    /// Toggled by the Runtime's distraction monitor. Entering/leaving flips the
    /// state machine over immediately rather than waiting for the current
    /// state's timer to expire, so the reaction reads as instant.
    func setDistracted(_ distracted: Bool) {
        guard distracted != isDistracted else { return }
        isDistracted = distracted
        stateEndTime = .distantPast
    }

    /// Toggled by the Runtime's gravity simulation each tick. Falling preempts
    /// every other state, including anger - a pet mid-tumble doesn't keep
    /// darting sideways.
    func setFalling(_ falling: Bool) {
        guard falling != isFalling else { return }
        isFalling = falling
        if !falling {
            stateEndTime = .distantPast // force a re-pick on landing
        }
    }

    /// Triggered by the Runtime whenever the pet gets fed - a brief, self-
    /// expiring celebration that preempts the normal idle/walk picker but
    /// yields to anything physically incompatible with dancing (being
    /// dragged or mid-fall).
    func celebrate(now: Date = Date()) {
        guard !isBeingDragged, !isFalling else { return }
        isDancing = true
        anim = .dance
        danceStartTime = now
        lastShimmyOffset = 0
        stateEndTime = now.addingTimeInterval(Self.danceDuration)
    }

    /// Advances the behavior state machine. Returns the horizontal distance (in
    /// points) to move this tick if walking/angry, else 0. Falling itself is
    /// purely vertical and driven by the Runtime's physics, not this dx.
    func tick(now: Date, mood: Mood) -> CGFloat {
        guard !isBeingDragged else { return 0 }

        if isFalling {
            anim = .fall
            return 0
        }

        if isDistracted {
            // Normally `Rampage` (driven by the Runtime) owns position while
            // distracted and this dx is discarded - but if the Accessibility
            // read couldn't get window geometry, there's no Rampage, and this
            // sprint-in-place-along-the-ledge is the fallback so the pet still
            // reacts visibly instead of just standing there looking angry.
            if now >= stateEndTime {
                anim = .angry
                facingRight.toggle() // dart back and forth rather than committing to one direction
                stateEndTime = now.addingTimeInterval(0.35 + rng() * 0.35)
            }
            return facingRight ? Self.angrySpeed : -Self.angrySpeed
        }

        if isDancing {
            if now >= stateEndTime {
                isDancing = false
                stateEndTime = .distantPast // force a re-pick now that the dance is over
            } else {
                anim = .dance
                // Groove side to side in place: a sine offset from the dance's
                // start, converted to a per-tick delta since `tick` returns a
                // relative move rather than an absolute position.
                let elapsed = now.timeIntervalSince(danceStartTime)
                let offset = CGFloat(sin(elapsed * Self.danceShimmyRate)) * Self.danceShimmyAmplitude
                let dx = offset - lastShimmyOffset
                lastShimmyOffset = offset
                return dx
            }
        }

        if mood == .tired && anim != .sleep {
            anim = .sleep
            stateEndTime = now.addingTimeInterval(3600) // "sleeps it off"; woken by stat recovery elsewhere
        } else if mood == .tired {
            // stay asleep
        } else if now >= stateEndTime {
            pickNextState(now: now, mood: mood)
        }

        switch anim {
        case .walk:
            return facingRight ? Self.walkSpeed : -Self.walkSpeed
        default:
            return 0
        }
    }

    /// Called externally once energy has recovered enough to end a sleep state early.
    func wake(now: Date) {
        if anim == .sleep {
            stateEndTime = .distantPast
        }
    }

    private func pickNextState(now: Date, mood: Mood) {
        if mood == .sad {
            anim = .sad
            stateEndTime = now.addingTimeInterval(2 + rng() * 2)
            return
        }

        // Weighted toward idle/sitting so the pet mostly holds still.
        let roll = rng()
        switch roll {
        case 0..<0.55:
            anim = .idle
            stateEndTime = now.addingTimeInterval(3 + rng() * 6)
        case 0.55..<0.9:
            anim = .walk
            facingRight = rng() > 0.5
            stateEndTime = now.addingTimeInterval(2 + rng() * 4)
        default:
            anim = .idle
            stateEndTime = now.addingTimeInterval(6 + rng() * 8)
        }
    }
}
