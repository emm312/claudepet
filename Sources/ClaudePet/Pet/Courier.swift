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
    private var handoffEndTime: Date = .distantPast
    private var lastTickDate: Date

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
    private static let arrivalEpsilon: CGFloat = 2

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
            }
        case .away:
            if now >= awayDeadline { phase = .returning }
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
    /// waiting out the full timeout when delivery actually succeeded.
    func receivedAck() {
        guard phase == .away else { return }
        phase = .returning
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
