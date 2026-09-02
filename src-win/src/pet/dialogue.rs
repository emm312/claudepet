//! Canned speech-bubble lines, pooled by mood, with no-immediate-repeat.
//! All lines are deliberately buzzword-poisoned corporate jargon.
//! Mirrors `Sources/ClaudePet/Pet/Dialogue.swift`.

use super::pet_state::Mood;
use rand::seq::SliceRandom;

const HAPPY: &[&str] = &[
    "crushing it synergistically",
    "hitting my KPIs today",
    "living my best-practice life",
    "10x-ing the vibes",
];
const CONTENT: &[&str] = &[
    "circling back to baseline",
    "low-key leveraging synergy",
    "*aligns on next steps*",
    "steady-state ideation",
];
const HUNGRY: &[&str] = &[
    "requesting more runway",
    "my hunger KPI is trending down",
    "need to fuel the growth engine",
    "feed me actionable nutrients",
];
const TIRED: &[&str] = &[
    "running on fumes, not scalable",
    "recharging my synergy battery",
    "taking a strategic power nap",
    "zzz... optimizing offline",
];
const SAD: &[&str] = &[
    "my morale metrics are down",
    "feeling a bit off-roadmap",
    "need a win to move the needle",
    "*disengaged stakeholder energy*",
];
const DIRTY: &[&str] = &[
    "requesting a full system refresh",
    "streamlining my hygiene stack",
    "ew, technical debt on my fur",
    "let's circle back on cleanliness",
];

const CELEBRATION: &[&str] = &[
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
];

const ANGRY: &[&str] = &[
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
];

const DEPART: &[&str] = &[
    "off to sync up cross-functionally",
    "taking this offline",
    "let's take this conversation async",
    "going to close the loop in person",
];

const DELIVERY_FAILED: &[&str] = &[
    "couldn't find them, going back to my desk",
    "no signal on that stakeholder, retrying later",
    "message bounced, circling back",
];

#[derive(Default)]
pub struct Dialogue {
    last_line: Option<&'static str>,
    last_celebration: Option<&'static str>,
    last_angry: Option<&'static str>,
}

fn pool_for(mood: Mood) -> &'static [&'static str] {
    match mood {
        Mood::Happy => HAPPY,
        Mood::Content => CONTENT,
        Mood::Hungry => HUNGRY,
        Mood::Tired => TIRED,
        Mood::Sad => SAD,
        Mood::Dirty => DIRTY,
    }
}

/// Pick from `pool`, avoiding `last` for up to 5 attempts (matches Swift).
fn pick_avoiding(pool: &'static [&'static str], last: Option<&'static str>) -> &'static str {
    let mut rng = rand::thread_rng();
    let mut candidate = *pool.choose(&mut rng).unwrap_or(&"...");
    if pool.len() > 1 {
        let mut attempts = 0;
        while Some(candidate) == last && attempts < 5 {
            candidate = *pool.choose(&mut rng).unwrap_or(&candidate);
            attempts += 1;
        }
    }
    candidate
}

impl Dialogue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn line(&mut self, mood: Mood) -> &'static str {
        let c = pick_avoiding(pool_for(mood), self.last_line);
        self.last_line = Some(c);
        c
    }

    pub fn celebration_line(&mut self) -> &'static str {
        let c = pick_avoiding(CELEBRATION, self.last_celebration);
        self.last_celebration = Some(c);
        c
    }

    pub fn angry_line(&mut self) -> &'static str {
        let c = pick_avoiding(ANGRY, self.last_angry);
        self.last_angry = Some(c);
        c
    }

    pub fn depart_line(&self) -> &'static str {
        rand::thread_rng()
            .gen_pick(DEPART)
            .unwrap_or("taking this offline")
    }

    pub fn delivery_failed_line(&self) -> &'static str {
        rand::thread_rng()
            .gen_pick(DELIVERY_FAILED)
            .unwrap_or("couldn't find them, going back to my desk")
    }
}

/// Whether `s` is one of the "couldn't deliver" lines - used by the runtime
/// tests to assert the failure bubble fires only on the timeout path.
#[cfg(test)]
pub fn is_delivery_failed_line(s: &str) -> bool {
    DELIVERY_FAILED.contains(&s)
}

/// Small helper so `depart_line`/`delivery_failed_line` read cleanly.
trait GenPick {
    fn gen_pick(&mut self, pool: &'static [&'static str]) -> Option<&'static str>;
}
impl GenPick for rand::rngs::ThreadRng {
    fn gen_pick(&mut self, pool: &'static [&'static str]) -> Option<&'static str> {
        pool.choose(self).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_come_from_the_right_pool() {
        let mut d = Dialogue::new();
        for _ in 0..20 {
            assert!(HUNGRY.contains(&d.line(Mood::Hungry)));
            assert!(CELEBRATION.contains(&d.celebration_line()));
            assert!(ANGRY.contains(&d.angry_line()));
            assert!(DEPART.contains(&d.depart_line()));
            assert!(DELIVERY_FAILED.contains(&d.delivery_failed_line()));
        }
    }

    #[test]
    fn no_immediate_repeat_across_many_draws() {
        let mut d = Dialogue::new();
        let mut prev = d.angry_line();
        for _ in 0..200 {
            let next = d.angry_line();
            assert_ne!(prev, next, "angry line repeated immediately");
            prev = next;
        }
    }
}
