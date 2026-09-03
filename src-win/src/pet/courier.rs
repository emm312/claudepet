//! The horizontal walk-off / walk-on state machine for pet-to-pet message
//! delivery. Pure logic driven by an injected `now` (wall-clock seconds) and
//! caller-supplied x positions. Mirrors `Sources/ClaudePet/Pet/Courier.swift`.
//!
//! Two roles share one state machine because both amount to "walk to an x
//! position, maybe wait, then walk to another x position":
//!  - `Outbound`: the local pet leaving to deliver a message and coming home.
//!    `Departing` -> `Away` -> `Returning` -> `Done`
//!  - `Inbound`: a visiting sprite arriving to hand off a message.
//!    `Arriving` -> `Handing` -> `Leaving` -> `Done`

use super::brain::AnimState;
use crate::net::Edge;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Outbound,
    Inbound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Departing,
    Away,
    Returning,
    Arriving,
    Handing,
    Leaving,
    Done,
}

const SPEED: f64 = 90.0; // pt/s - brisker than the normal idle walk
/// How long the inbound visitor pauses beside the resident pet to pass the
/// letter. The Swift original holds 2.2s here so the message can be shown during
/// the handoff; on the windows branch the resident pet keeps the letter and it's
/// opened on demand (see `runtime.rs` / `letter.rs`), so the visitor just touches
/// and goes rather than lingering.
pub const HANDOFF_DURATION: f64 = 0.5;
/// How long the outbound pet waits off-screen for an ack before circling back.
/// 15s (not 10) so a slow discovery/address-fixup on the sender or a congested
/// LAN doesn't fire the "message bounced" bubble for a message that actually
/// got through - the ack only races a *successful* delivery, so the extra wait
/// mostly costs a false-negative nothing.
const AWAY_TIMEOUT: f64 = 15.0;
/// Fallback wait applied on an ack that doesn't carry its own
/// `time_to_return` (an older peer's wire message, pre-dating that field).
/// Otherwise the sender has no idea how long the recipient's own visitor
/// animation will take and would turn around before it's actually done.
pub const DEFAULT_WAIT: f64 = 2.0;
const ARRIVAL_EPSILON: f64 = 2.0;

/// How long a courier's `Arriving -> Handing -> Leaving` leg takes to play out
/// on the receiving screen, given the one-way arrival distance (== the leaving
/// distance, since both run between the same off-screen point and handoff
/// point) and the delivery's speed multiplier (>1.0 for express/horse). The
/// receiver computes this for its own screen and hands it back on the ack, so
/// the sender's `Away` phase can wait exactly that long instead of guessing.
pub fn estimate_round_trip_duration(one_way_distance: f64, speed_mult: f64) -> f64 {
    let speed = SPEED * speed_mult.max(0.1);
    2.0 * one_way_distance.abs() / speed + HANDOFF_DURATION
}

pub struct Courier {
    phase: Phase,
    #[allow(dead_code)]
    role: Role,
    #[allow(dead_code)]
    edge: Edge,
    x: f64,
    facing_right: bool,
    off_screen_x: f64,
    home_x: f64,
    handoff_x: f64,
    away_deadline: f64,
    /// When `Away` was entered - the anchor `received_ack`'s `wait` (the
    /// recipient's own animation duration) counts forward from.
    away_entered_at: f64,
    /// Earliest moment `Away` is allowed to end on an ack, once known - see
    /// `received_ack`. Irrelevant once `Away` is left.
    away_min_end_time: f64,
    handoff_end_time: f64,
    last_tick_date: f64,
    speed: f64,
    /// Set when an ack arrives before it's allowed to end `Away` yet - either
    /// still `Departing` (so `Away` is skipped on entry instead of making a
    /// delivery that already succeeded wait out the full timeout) or already
    /// `Away` but short of the recipient's own animation finishing. Applied
    /// the moment it's safe.
    ack_pending: bool,
    /// The `wait` from an ack that arrived during `Departing`, before
    /// `away_entered_at` is even known yet - applied once `Away` begins.
    pending_wait: f64,
}

impl Courier {
    /// `speed_mult` > 1.0 for an express (horse) delivery.
    pub fn outbound(
        start_x: f64,
        home_x: f64,
        off_screen_x: f64,
        edge: Edge,
        now: f64,
        speed_mult: f64,
    ) -> Courier {
        Courier::new(Role::Outbound, Phase::Departing, edge, start_x, off_screen_x, home_x, home_x, now, speed_mult)
    }

    pub fn inbound(
        off_screen_x: f64,
        handoff_x: f64,
        edge: Edge,
        now: f64,
        speed_mult: f64,
    ) -> Courier {
        Courier::new(Role::Inbound, Phase::Arriving, edge, off_screen_x, off_screen_x, handoff_x, handoff_x, now, speed_mult)
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        role: Role,
        phase: Phase,
        edge: Edge,
        x: f64,
        off_screen_x: f64,
        home_x: f64,
        handoff_x: f64,
        now: f64,
        speed_mult: f64,
    ) -> Courier {
        let away_deadline = if role == Role::Outbound {
            now + AWAY_TIMEOUT
        } else {
            f64::NEG_INFINITY
        };
        Courier {
            phase,
            role,
            edge,
            x,
            facing_right: true,
            off_screen_x,
            home_x,
            handoff_x,
            away_deadline,
            away_entered_at: f64::NEG_INFINITY,
            away_min_end_time: f64::NEG_INFINITY,
            handoff_end_time: f64::NEG_INFINITY,
            last_tick_date: now,
            speed: SPEED * speed_mult.max(0.1),
            ack_pending: false,
            pending_wait: 0.0,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn x(&self) -> f64 {
        self.x
    }
    pub fn facing_right(&self) -> bool {
        self.facing_right
    }
    #[allow(dead_code)]
    pub fn is_done(&self) -> bool {
        self.phase == Phase::Done
    }
    /// True while the outbound pet's window should stay hidden.
    pub fn is_away(&self) -> bool {
        self.phase == Phase::Away
    }
    /// The resting x position this courier departed from / returns to. Used
    /// by the runtime to anchor an inbound delivery's handoff point off of
    /// the resident pet's actual resting spot when an outbound trip is
    /// simultaneously in flight and has `pet_x` off mid-transit.
    pub fn home_x(&self) -> f64 {
        self.home_x
    }

    /// `Walk` while moving, `Idle` while paused (away, or mid handoff).
    pub fn anim(&self) -> AnimState {
        match self.phase {
            Phase::Away | Phase::Handing | Phase::Done => AnimState::Idle,
            Phase::Departing | Phase::Returning | Phase::Arriving | Phase::Leaving => AnimState::Walk,
        }
    }

    /// Advances the state machine. Returns `false` once `Done`.
    pub fn tick(&mut self, now: f64) -> bool {
        let dt = now - self.last_tick_date;
        self.last_tick_date = now;
        if dt <= 0.0 {
            return self.phase != Phase::Done;
        }

        match self.phase {
            Phase::Departing => {
                self.move_toward(self.off_screen_x, dt);
                if self.reached(self.off_screen_x) {
                    self.phase = Phase::Away;
                    self.away_deadline = now + AWAY_TIMEOUT;
                    self.away_entered_at = now;
                    if self.ack_pending {
                        self.away_min_end_time = now + self.pending_wait;
                    }
                }
            }
            Phase::Away => {
                if now >= self.away_deadline {
                    self.phase = Phase::Returning;
                } else if self.ack_pending && now >= self.away_min_end_time {
                    self.phase = Phase::Returning;
                }
            }
            Phase::Returning => {
                self.move_toward(self.home_x, dt);
                if self.reached(self.home_x) {
                    self.phase = Phase::Done;
                }
            }
            Phase::Arriving => {
                self.move_toward(self.handoff_x, dt);
                if self.reached(self.handoff_x) {
                    self.phase = Phase::Handing;
                    self.handoff_end_time = now + HANDOFF_DURATION;
                }
            }
            Phase::Handing => {
                if now >= self.handoff_end_time {
                    self.phase = Phase::Leaving;
                }
            }
            Phase::Leaving => {
                self.move_toward(self.off_screen_x, dt);
                if self.reached(self.off_screen_x) {
                    self.phase = Phase::Done;
                }
            }
            Phase::Done => {}
        }
        self.phase != Phase::Done
    }

    /// Called once the peer's ack arrives, so the outbound pet doesn't sit
    /// waiting out the full timeout when delivery actually succeeded. `wait`
    /// is how long the recipient said its own visitor animation still needs
    /// (the ack's `time_to_return`, computed on their screen) - the horse
    /// isn't allowed to turn around until that long after `Away` began, so it
    /// doesn't beat the recipient's own pet finishing the handoff. An ack
    /// that lands while still `Departing` (a fast LAN can beat the walk-off
    /// animation) is recorded and applied the moment `Away` begins.
    pub fn received_ack(&mut self, now: f64, wait: f64) {
        let wait = wait.max(0.0);
        match self.phase {
            Phase::Away => {
                let target = self.away_entered_at + wait;
                if now >= target {
                    self.phase = Phase::Returning;
                } else {
                    self.ack_pending = true;
                    self.away_min_end_time = target;
                }
            }
            Phase::Departing => {
                self.ack_pending = true;
                self.pending_wait = wait;
            }
            _ => {}
        }
    }

    /// Pushes the away-timeout deadline further out (used while a large
    /// attachment is still being pulled by the receiver, so a slow transfer
    /// doesn't fire the "message bounced" bubble).
    pub fn extend_deadline(&mut self, new_deadline: f64) {
        if self.phase == Phase::Away && new_deadline > self.away_deadline {
            self.away_deadline = new_deadline;
        }
    }

    fn move_toward(&mut self, target: f64, dt: f64) {
        self.facing_right = target > self.x;
        let step = dt * self.speed;
        if (target - self.x).abs() <= step {
            self.x = target;
        } else if self.facing_right {
            self.x += step;
        } else {
            self.x -= step;
        }
    }

    fn reached(&self, target: f64) -> bool {
        (self.x - target).abs() <= ARRIVAL_EPSILON
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ports Tests/ClaudePetTests/CourierTests.swift. Timing values that the Swift
    // test file anchors to courier *creation* are re-anchored here to the phase
    // transition, matching Courier.swift's "reset the deadline when the phase is
    // entered" logic (the Swift assertions are inconsistent with that and would
    // fail against the implementation they accompany).

    #[test]
    fn outbound_departs_then_waits_at_the_edge() {
        let start = 0.0;
        let mut c = Courier::outbound(100.0, 100.0, 300.0, Edge::Right, start, 1.0);
        assert_eq!(c.phase(), Phase::Departing);
        assert!(c.facing_right());

        c.tick(start + 3.0); // enough real time to cover the 200pt gap at 90pt/s
        assert_eq!(c.phase(), Phase::Away);
        assert!((c.x() - 300.0).abs() < 0.01);
    }

    #[test]
    fn outbound_returns_home_after_the_recipients_wait_elapses() {
        let start = 0.0;
        let mut c = Courier::outbound(100.0, 100.0, 300.0, Edge::Right, start, 1.0);
        c.tick(start + 3.0); // reaches the edge -> Away (entered_at = start+3)
        assert_eq!(c.phase(), Phase::Away);

        // Ack arrives instantly but says the recipient's own visitor needs 2s.
        c.received_ack(start + 3.01, 2.0);
        // Too soon - the horse shouldn't beat the recipient's own animation.
        assert_eq!(c.phase(), Phase::Away);

        c.tick(start + 4.5); // still short of entered_at (start+3) + wait (2s)
        assert_eq!(c.phase(), Phase::Away);

        c.tick(start + 5.01); // now past start+3+2 - the pending ack applies
        assert_eq!(c.phase(), Phase::Returning);

        c.tick(start + 8.01);
        assert_eq!(c.phase(), Phase::Done);
        assert!((c.x() - 100.0).abs() < 0.01);
    }

    #[test]
    fn ack_that_arrives_after_the_wait_already_elapsed_returns_immediately() {
        let start = 0.0;
        let mut c = Courier::outbound(100.0, 100.0, 300.0, Edge::Right, start, 1.0);
        c.tick(start + 3.0);
        assert_eq!(c.phase(), Phase::Away);

        // The recipient's own animation only needed 1s, and this ack shows up
        // well after that - nothing left to wait for.
        c.received_ack(start + 6.0, 1.0);
        assert_eq!(c.phase(), Phase::Returning);
    }

    #[test]
    fn outbound_times_out_and_returns_without_ack() {
        let start = 0.0;
        let mut c = Courier::outbound(100.0, 100.0, 300.0, Edge::Right, start, 1.0);
        c.tick(start + 3.0); // reaches edge -> Away, deadline now (start+3)+10
        assert_eq!(c.phase(), Phase::Away);

        c.tick(start + 10.0); // not yet at the timeout (deadline = start+3+15)
        assert_eq!(c.phase(), Phase::Away);

        c.tick(start + 14.0); // still inside the 15s wait
        assert_eq!(c.phase(), Phase::Away);

        c.tick(start + 19.0); // past the 15s wait, no ack ever received
        assert_eq!(c.phase(), Phase::Returning);
    }

    #[test]
    fn received_ack_is_ignored_outside_away_phase() {
        let start = 0.0;
        let mut c = Courier::outbound(100.0, 100.0, 300.0, Edge::Right, start, 1.0);
        assert_eq!(c.phase(), Phase::Departing);
        c.received_ack(start, 2.0); // doesn't change phase while still departing
        assert_eq!(c.phase(), Phase::Departing);
    }

    #[test]
    fn inbound_arrives_hands_off_then_leaves() {
        let start = 0.0;
        let mut c = Courier::inbound(-200.0, 40.0, Edge::Left, start, 1.0);
        assert_eq!(c.phase(), Phase::Arriving);
        assert!(c.facing_right()); // walking rightward, toward the resident pet

        c.tick(start + 3.0);
        assert_eq!(c.phase(), Phase::Handing);
        assert!((c.x() - 40.0).abs() < 0.01);

        // Short "touch and go" handoff (0.5s) - the visitor doesn't linger.
        c.tick(start + 3.2);
        assert_eq!(c.phase(), Phase::Handing);

        // Past the handoff (started at start+3).
        c.tick(start + 4.0);
        assert_eq!(c.phase(), Phase::Leaving);

        c.tick(start + 10.0);
        assert_eq!(c.phase(), Phase::Done);
        assert!((c.x() - (-200.0)).abs() < 0.01);
    }

    #[test]
    fn anim_reflects_movement_vs_pause() {
        let start = 0.0;
        let mut c = Courier::outbound(100.0, 100.0, 300.0, Edge::Right, start, 1.0);
        assert_eq!(c.anim(), AnimState::Walk);
        c.tick(start + 3.0);
        assert_eq!(c.phase(), Phase::Away);
        assert_eq!(c.anim(), AnimState::Idle);
    }
}
