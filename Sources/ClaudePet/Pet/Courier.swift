import Foundation
import CoreGraphics

/// Drives the horizontal walk-off/walk-on animation for pet-to-pet message
/// delivery. Pure logic (no AppKit windows), driven by an injected `now` and
/// starting/target x positions supplied by the caller - unit-testable exactly
/// like `Brain` and `PetState`.
///
/// Two roles share one state machine because both amount to "walk to an x
/// position, maybe wait, then walk to another x position":
///
/// - `.outbound`: the local pet leaving to deliver a message and coming home.
///   `.departing` -> `.away` -> `.returning` -> `.done`
/// - `.inbound`: a visiting sprite arriving to hand off a message.
///   `.arriving` -> `.handing` -> `.leaving` -> `.done`
final class Courier {
    enum Role { case outbound, inbound }

    enum Phase: Equatable {
        case departing, away, returning // outbound
        case arriving, handing, leaving // inbound
        case done
    }

    private(set) var phase: Phase
    let role: Role
    let edge: PetMessage.Edge
    /// Express (horse) delivery - the courier moves at `expressSpeedMultiplier`
    /// times the normal pace. Purely a speed/rendering flag; the state machine
    /// itself is unchanged.
    let express: Bool

    private(set) var x: CGFloat
    private(set) var facingRight = true

    private let offScreenX: CGFloat
    private let homeX: CGFloat
    private let handoffX: CGFloat

    private var awayDeadline: Date = .distantPast
    /// When `.away` was entered - the anchor `receivedAck`'s `wait` (the
    /// recipient's own animation duration) counts forward from.
    private var awayEnteredAt: Date = .distantPast
    /// Earliest moment `.away` is allowed to end on an ack, once known - see
    /// `receivedAck`. Irrelevant once `.away` is left.
    private var awayMinEndTime: Date = .distantPast
    private var handoffEndTime: Date = .distantPast
    private var lastTickDate: Date
    /// Set when an ack arrives before it's allowed to end `.away` yet - either
    /// still `.departing` (so `.away` is skipped on entry instead of making a
    /// delivery that already succeeded wait out the full timeout) or already
    /// `.away` but short of the recipient's own animation finishing. Applied
    /// the moment it's safe.
    private var ackPending = false
    /// The `wait` from an ack that arrived during `.departing`, before
    /// `awayEnteredAt` is even known yet - applied once `.away` begins.
    private var pendingWait: TimeInterval = 0

    private static let speed: CGFloat = 90 // pt/s - brisker than the normal idle walk
    static let expressSpeedMultiplier: CGFloat = 3.0
    private var effectiveSpeed: CGFloat { express ? Self.speed * Self.expressSpeedMultiplier : Self.speed }
    /// How long the inbound visitor pauses beside the resident pet to pass the
    /// letter. Was 2.2s so the message could be shown during the handoff; now
    /// the resident pet keeps the letter and it's opened on demand (see
    /// `Runtime.handlePet` / `LetterWindow`), so the visitor just touches and
    /// goes rather than lingering. Matches `HANDOFF_DURATION` in the Windows port.
    static let handoffDuration: TimeInterval = 0.5
    /// How long the outbound pet waits off-screen for an ack before circling
    /// back. 15s (not 10) so a slow discovery/address-fixup on the sender or a
    /// congested LAN doesn't fire the "message bounced" bubble for a message
    /// that actually got through - the ack only races a *successful* delivery,
    /// so the extra wait mostly costs a false-negative nothing.
    static let awayTimeout: TimeInterval = 15
    /// Fallback wait applied on an ack that doesn't carry its own
    /// `timeToReturn` (an older peer's wire message, pre-dating that field).
    /// Otherwise the sender has no idea how long the recipient's own visitor
    /// animation will take and would turn around before it's actually done.
    /// Matches `DEFAULT_WAIT` in the Windows port.
    static let defaultWait: TimeInterval = 2
    private static let arrivalEpsilon: CGFloat = 2

    /// How long a courier's `.arriving -> .handing -> .leaving` leg takes to
    /// play out on the receiving screen, given the one-way arrival distance
    /// (== the leaving distance, since both run between the same off-screen
    /// point and handoff point) and whether it's express (horse). The
    /// receiver computes this for its own screen and hands it back on the
    /// ack, so the sender's `.away` phase can wait exactly that long instead
    /// of guessing. Matches `estimate_round_trip_duration` in the Windows port.
    static func estimateRoundTripDuration(oneWayDistance: CGFloat, express: Bool) -> TimeInterval {
        let effective = express ? speed * expressSpeedMultiplier : speed
        return 2 * Double(abs(oneWayDistance) / effective) + handoffDuration
    }

    /// `.walk` while moving, `.idle` while paused (away, or mid handoff).
    var anim: PetMood.AnimState {
        switch phase {
        case .away, .handing, .done: return .idle
        case .departing, .returning, .arriving, .leaving: return .walk
        }
    }

    var isDone: Bool { phase == .done }
    /// True while the outbound pet's window should stay hidden - it's off
    /// delivering and hasn't started walking back yet.
    var isAway: Bool { phase == .away }
    /// The resting x position this courier departed from / returns to. Used
    /// by the runtime to anchor an inbound delivery's handoff point off of
    /// the resident pet's actual resting spot when an outbound trip is
    /// simultaneously in flight and the pet's live position is mid-transit.
    var restingX: CGFloat { homeX }

    static func outbound(startX: CGFloat, homeX: CGFloat, offScreenX: CGFloat, edge: PetMessage.Edge, express: Bool = false, now: Date = Date()) -> Courier {
        Courier(role: .outbound, phase: .departing, edge: edge, x: startX, offScreenX: offScreenX, homeX: homeX, handoffX: homeX, express: express, now: now)
    }

    static func inbound(offScreenX: CGFloat, handoffX: CGFloat, edge: PetMessage.Edge, express: Bool = false, now: Date = Date()) -> Courier {
        Courier(role: .inbound, phase: .arriving, edge: edge, x: offScreenX, offScreenX: offScreenX, homeX: handoffX, handoffX: handoffX, express: express, now: now)
    }

    private init(role: Role, phase: Phase, edge: PetMessage.Edge, x: CGFloat, offScreenX: CGFloat, homeX: CGFloat, handoffX: CGFloat, express: Bool, now: Date) {
        self.role = role
        self.phase = phase
        self.edge = edge
        self.x = x
        self.offScreenX = offScreenX
        self.homeX = homeX
        self.handoffX = handoffX
        self.express = express
        lastTickDate = now
        if role == .outbound {
            awayDeadline = now.addingTimeInterval(Self.awayTimeout)
        }
    }

    /// Advances the state machine. Returns `false` once `.done`, so callers can
    /// `while courier.tick(now: now) { }`-style poll a single step per frame.
    @discardableResult
    func tick(now: Date) -> Bool {
        let dt = now.timeIntervalSince(lastTickDate)
        lastTickDate = now
        guard dt > 0 else { return phase != .done }

        switch phase {
        case .departing:
            moveToward(offScreenX, dt: dt)
            if reached(offScreenX) {
                phase = .away
                awayDeadline = now.addingTimeInterval(Self.awayTimeout)
                awayEnteredAt = now
                if ackPending {
                    awayMinEndTime = now.addingTimeInterval(pendingWait)
                }
            }
        case .away:
            if now >= awayDeadline {
                phase = .returning
            } else if ackPending && now >= awayMinEndTime {
                phase = .returning
            }
        case .returning:
            moveToward(homeX, dt: dt)
            if reached(homeX) { phase = .done }
        case .arriving:
            moveToward(handoffX, dt: dt)
            if reached(handoffX) {
                phase = .handing
                handoffEndTime = now.addingTimeInterval(Self.handoffDuration)
            }
        case .handing:
            if now >= handoffEndTime { phase = .leaving }
        case .leaving:
            moveToward(offScreenX, dt: dt)
            if reached(offScreenX) { phase = .done }
        case .done:
            break
        }
        return phase != .done
    }

    /// Called once the peer's ack arrives, so the outbound pet doesn't sit
    /// waiting out the full timeout when delivery actually succeeded. `wait`
    /// is how long the recipient said its own visitor animation still needs
    /// (the ack's `timeToReturn`, computed on their screen) - the horse isn't
    /// allowed to turn around until that long after `.away` began, so it
    /// doesn't beat the recipient's own pet finishing the handoff. An ack
    /// that lands while still `.departing` (a fast LAN can beat the walk-off
    /// animation) is recorded and applied the moment `.away` begins.
    func receivedAck(wait: TimeInterval, now: Date = Date()) {
        let wait = max(wait, 0)
        switch phase {
        case .away:
            let target = awayEnteredAt.addingTimeInterval(wait)
            if now >= target {
                phase = .returning
            } else {
                ackPending = true
                awayMinEndTime = target
            }
        case .departing:
            ackPending = true
            pendingWait = wait
        default: break
        }
    }

    /// Pushes the away-timeout deadline further out (used while a large
    /// attachment is still being pulled by the receiver, so a slow transfer
    /// doesn't fire the "message bounced" bubble).
    func extendDeadline(to newDeadline: Date) {
        guard phase == .away, newDeadline > awayDeadline else { return }
        awayDeadline = newDeadline
    }

    private func moveToward(_ target: CGFloat, dt: TimeInterval) {
        facingRight = target > x
        let step = CGFloat(dt) * effectiveSpeed
        if abs(target - x) <= step {
            x = target
        } else {
            x += facingRight ? step : -step
        }
    }

    private func reached(_ target: CGFloat) -> Bool {
        abs(x - target) <= Self.arrivalEpsilon
    }
}
