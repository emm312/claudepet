//! The pet's behaviour state machine - what it's *doing* on screen (idle / walk /
//! sleep / dance / angry / fall), as opposed to `Mood` which is about stats.
//! Mirrors `Sources/ClaudePet/Pet/Brain.swift`.
//!
//! All timestamps are wall-clock seconds (f64); `f64::NEG_INFINITY` stands in
//! for Swift's `Date.distantPast` ("force a re-pick next tick").

use super::pet_state::Mood;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimState {
    Idle,
    Walk,
    Sleep,
    Sad,
    Dragged,
    Angry,
    Fall,
    Dance,
}

const WALK_SPEED: f64 = 1.2;
const ANGRY_SPEED: f64 = 4.5;
const DANCE_DURATION: f64 = 2.6;
const DANCE_SHIMMY_AMPLITUDE: f64 = 6.0;
const DANCE_SHIMMY_RATE: f64 = 5.5; // radians/sec

pub struct Brain {
    anim: AnimState,
    facing_right: bool,
    state_end_time: f64,
    is_being_dragged: bool,
    is_distracted: bool,
    is_falling: bool,
    is_dancing: bool,
    dance_start_time: f64,
    last_shimmy_offset: f64,
    rng: Box<dyn FnMut() -> f64 + Send>,
}

impl Brain {
    pub fn new() -> Self {
        Self::with_rng(Box::new(|| rand::random::<f64>()))
    }

    pub fn with_rng(rng: Box<dyn FnMut() -> f64 + Send>) -> Self {
        Brain {
            anim: AnimState::Idle,
            facing_right: true,
            state_end_time: f64::NEG_INFINITY,
            is_being_dragged: false,
            is_distracted: false,
            is_falling: false,
            is_dancing: false,
            dance_start_time: f64::NEG_INFINITY,
            last_shimmy_offset: 0.0,
            rng,
        }
    }

    pub fn anim(&self) -> AnimState {
        self.anim
    }
    pub fn facing_right(&self) -> bool {
        self.facing_right
    }
    pub fn is_distracted(&self) -> bool {
        self.is_distracted
    }
    #[allow(dead_code)]
    pub fn is_falling(&self) -> bool {
        self.is_falling
    }

    pub fn begin_drag(&mut self) {
        self.is_being_dragged = true;
        self.is_dancing = false;
        self.anim = AnimState::Dragged;
    }

    pub fn end_drag(&mut self) {
        self.is_being_dragged = false;
        self.state_end_time = f64::NEG_INFINITY; // force a re-pick next tick
    }

    /// Entering/leaving flips the state machine over immediately rather than
    /// waiting for the current state's timer to expire.
    pub fn set_distracted(&mut self, distracted: bool) {
        if distracted == self.is_distracted {
            return;
        }
        self.is_distracted = distracted;
        self.state_end_time = f64::NEG_INFINITY;
    }

    /// Falling preempts every other state, including anger.
    pub fn set_falling(&mut self, falling: bool) {
        if falling == self.is_falling {
            return;
        }
        self.is_falling = falling;
        if !falling {
            self.state_end_time = f64::NEG_INFINITY; // force a re-pick on landing
        }
    }

    /// A brief, self-expiring celebration (fed the pet) that preempts the normal
    /// idle/walk picker but yields to being dragged or mid-fall.
    pub fn celebrate(&mut self, now: f64) {
        if self.is_being_dragged || self.is_falling {
            return;
        }
        self.is_dancing = true;
        self.anim = AnimState::Dance;
        self.dance_start_time = now;
        self.last_shimmy_offset = 0.0;
        self.state_end_time = now + DANCE_DURATION;
    }

    /// Advances the behaviour state machine. Returns the horizontal distance (in
    /// points) to move this tick if walking/angry/dancing, else 0.
    pub fn tick(&mut self, now: f64, mood: Mood) -> f64 {
        if self.is_being_dragged {
            return 0.0;
        }

        if self.is_falling {
            self.anim = AnimState::Fall;
            return 0.0;
        }

        if self.is_distracted {
            if now >= self.state_end_time {
                self.anim = AnimState::Angry;
                self.facing_right = !self.facing_right; // dart back and forth
                self.state_end_time = now + 0.35 + (self.rng)() * 0.35;
            }
            return if self.facing_right {
                ANGRY_SPEED
            } else {
                -ANGRY_SPEED
            };
        }

        if self.is_dancing {
            if now >= self.state_end_time {
                self.is_dancing = false;
                self.state_end_time = f64::NEG_INFINITY; // force a re-pick now
            } else {
                self.anim = AnimState::Dance;
                // Groove side to side in place: a sine offset from the dance's
                // start, converted to a per-tick delta.
                let elapsed = now - self.dance_start_time;
                let offset = (elapsed * DANCE_SHIMMY_RATE).sin() * DANCE_SHIMMY_AMPLITUDE;
                let dx = offset - self.last_shimmy_offset;
                self.last_shimmy_offset = offset;
                return dx;
            }
        }

        if mood == Mood::Tired && self.anim != AnimState::Sleep {
            self.anim = AnimState::Sleep;
            self.state_end_time = now + 3600.0; // woken by stat recovery elsewhere
        } else if mood == Mood::Tired {
            // stay asleep
        } else if now >= self.state_end_time {
            self.pick_next_state(now, mood);
        }

        match self.anim {
            AnimState::Walk => {
                if self.facing_right {
                    WALK_SPEED
                } else {
                    -WALK_SPEED
                }
            }
            _ => 0.0,
        }
    }

    /// Called externally once energy has recovered enough to end a sleep early.
    pub fn wake(&mut self, _now: f64) {
        if self.anim == AnimState::Sleep {
            self.state_end_time = f64::NEG_INFINITY;
        }
    }

    fn pick_next_state(&mut self, now: f64, mood: Mood) {
        if mood == Mood::Sad {
            self.anim = AnimState::Sad;
            self.state_end_time = now + 2.0 + (self.rng)() * 2.0;
            return;
        }

        // Weighted toward idle/sitting so the pet mostly holds still.
        let roll = (self.rng)();
        if roll < 0.55 {
            self.anim = AnimState::Idle;
            self.state_end_time = now + 3.0 + (self.rng)() * 6.0;
        } else if roll < 0.9 {
            self.anim = AnimState::Walk;
            self.facing_right = (self.rng)() > 0.5;
            self.state_end_time = now + 2.0 + (self.rng)() * 4.0;
        } else {
            self.anim = AnimState::Idle;
            self.state_end_time = now + 6.0 + (self.rng)() * 8.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic RNG feeding a fixed sequence (looping), for state-machine tests.
    fn seq_rng(values: Vec<f64>) -> Box<dyn FnMut() -> f64 + Send> {
        let mut i = 0;
        Box::new(move || {
            let v = values[i % values.len()];
            i += 1;
            v
        })
    }

    #[test]
    fn falling_preempts_and_returns_zero() {
        let mut b = Brain::with_rng(seq_rng(vec![0.0]));
        b.set_falling(true);
        assert_eq!(b.tick(0.0, Mood::Content), 0.0);
        assert_eq!(b.anim(), AnimState::Fall);
    }

    #[test]
    fn dragging_preempts_everything() {
        let mut b = Brain::with_rng(seq_rng(vec![0.0]));
        b.begin_drag();
        assert_eq!(b.tick(1.0, Mood::Happy), 0.0);
        assert_eq!(b.anim(), AnimState::Dragged);
    }

    #[test]
    fn distracted_darts_back_and_forth_at_angry_speed() {
        let mut b = Brain::with_rng(seq_rng(vec![0.0]));
        b.set_distracted(true);
        let dx1 = b.tick(0.0, Mood::Content);
        assert_eq!(b.anim(), AnimState::Angry);
        assert_eq!(dx1.abs(), ANGRY_SPEED);
        // Immediately after, before the sub-second timer expires, keeps the same
        // direction (no toggle).
        let dx2 = b.tick(0.1, Mood::Content);
        assert_eq!(dx1, dx2);
    }

    #[test]
    fn tired_mood_sleeps() {
        let mut b = Brain::with_rng(seq_rng(vec![0.5]));
        b.tick(0.0, Mood::Tired);
        assert_eq!(b.anim(), AnimState::Sleep);
    }

    #[test]
    fn idle_branch_when_roll_below_055() {
        let mut b = Brain::with_rng(seq_rng(vec![0.1]));
        let dx = b.tick(0.0, Mood::Content);
        assert_eq!(b.anim(), AnimState::Idle);
        assert_eq!(dx, 0.0);
    }

    #[test]
    fn walk_branch_moves_horizontally() {
        // first rng() -> roll (0.7 => walk), second -> facing (0.9 => right), third -> duration
        let mut b = Brain::with_rng(seq_rng(vec![0.7, 0.9, 0.0]));
        let dx = b.tick(0.0, Mood::Content);
        assert_eq!(b.anim(), AnimState::Walk);
        assert_eq!(dx, WALK_SPEED);
    }

    #[test]
    fn celebrate_enters_dance_then_expires() {
        let mut b = Brain::with_rng(seq_rng(vec![0.1]));
        b.celebrate(0.0);
        assert_eq!(b.anim(), AnimState::Dance);
        b.tick(1.0, Mood::Content);
        assert_eq!(b.anim(), AnimState::Dance);
        // After DANCE_DURATION the dance ends and a normal state is picked.
        b.tick(DANCE_DURATION + 0.1, Mood::Content);
        assert_ne!(b.anim(), AnimState::Dance);
    }

    #[test]
    fn celebrate_ignored_while_dragged() {
        let mut b = Brain::with_rng(seq_rng(vec![0.0]));
        b.begin_drag();
        b.celebrate(0.0);
        assert_eq!(b.anim(), AnimState::Dragged);
    }
}
