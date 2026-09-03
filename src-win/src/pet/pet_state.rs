//! Persisted stat block for the pet + its JSON store.
//! Mirrors `Sources/ClaudePet/Pet/PetState.swift`.

use crate::pet::sprites::{AccessoryId, SkinId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

/// Discrete mood buckets, derived from stats, that drive sprite choice, movement
/// speed, and which dialogue pool to draw from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mood {
    Happy,
    Content,
    Hungry,
    Tired,
    Sad,
    Dirty,
}

/// Wall-clock seconds since the Unix epoch. The Swift app persists Foundation
/// `Date`s; this port keeps its own local `state.json` so the representation is
/// just a plain f64 and need not match Foundation's reference-date encoding.
pub fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

const HUNGER_DECAY_PER_HOUR: f64 = 3.0;
const ENERGY_DECAY_PER_HOUR: f64 = 2.0;
const HAPPINESS_DECAY_PER_HOUR: f64 = 1.5;
const CLEANLINESS_DECAY_PER_HOUR: f64 = 1.0;

/// Never simulate more than this much elapsed time in one jump, so returning
/// after a week away doesn't nuke every stat to zero in one tick.
const MAX_CATCH_UP: f64 = 12.0 * 3600.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PetState {
    /// 100 = full, 0 = starving
    pub hunger: f64,
    /// 100 = well rested, 0 = exhausted
    pub energy: f64,
    /// 100 = delighted, 0 = miserable
    pub happiness: f64,
    /// 100 = spotless, 0 = filthy
    pub cleanliness: f64,

    pub birth_date: f64,
    pub last_tick: f64,

    /// Install updates automatically in the background. Not pet state, but it
    /// rides along in the same `state.json` rather than a second config file.
    #[serde(default = "default_true")]
    pub auto_update: bool,

    /// The pet's chosen look and worn accessories. Rides along in the same
    /// `state.json` (see `auto_update` above for the precedent), defaulted so
    /// a `state.json` saved before skins existed still decodes as
    /// `SkinId::Classic` / no accessories instead of failing.
    #[serde(default)]
    pub skin: SkinId,
    #[serde(default)]
    pub accessories: HashSet<AccessoryId>,
}

fn default_true() -> bool {
    true
}

impl Default for PetState {
    fn default() -> Self {
        let now = now_secs();
        PetState {
            hunger: 80.0,
            energy: 100.0,
            happiness: 80.0,
            cleanliness: 100.0,
            birth_date: now,
            last_tick: now,
            auto_update: true,
            skin: SkinId::default(),
            accessories: HashSet::new(),
        }
    }
}

fn clamp(value: f64) -> f64 {
    value.clamp(0.0, 100.0)
}

impl PetState {
    /// Advances stats to `now`, based on elapsed wall-clock time since `last_tick`.
    /// Safe against clock skew: a negative or absurd delta is treated as zero.
    pub fn tick(&mut self, now: f64) {
        let mut elapsed = now - self.last_tick;
        if !elapsed.is_finite() || elapsed < 0.0 {
            elapsed = 0.0;
        }
        elapsed = elapsed.min(MAX_CATCH_UP);
        let hours = elapsed / 3600.0;

        self.hunger = clamp(self.hunger - HUNGER_DECAY_PER_HOUR * hours);
        self.energy = clamp(self.energy - ENERGY_DECAY_PER_HOUR * hours);
        self.happiness = clamp(self.happiness - HAPPINESS_DECAY_PER_HOUR * hours);
        self.cleanliness = clamp(self.cleanliness - CLEANLINESS_DECAY_PER_HOUR * hours);

        self.last_tick = now;
    }

    pub fn feed(&mut self) {
        self.hunger = clamp(self.hunger + 30.0);
        self.happiness = clamp(self.happiness + 5.0);
    }

    pub fn play(&mut self) {
        self.happiness = clamp(self.happiness + 20.0);
        self.energy = clamp(self.energy - 10.0);
        self.hunger = clamp(self.hunger - 5.0);
    }

    pub fn clean(&mut self) {
        self.cleanliness = 100.0;
    }

    pub fn pet(&mut self) {
        self.happiness = clamp(self.happiness + 8.0);
    }

    #[allow(dead_code)]
    pub fn sleep(&mut self, hours: f64) {
        self.energy = clamp(self.energy + hours * 12.0);
    }

    /// Overall mood derived from the worst-off relevant stat, in priority order -
    /// a starving pet reads as hungry even if it's also a bit dirty.
    pub fn mood(&self) -> Mood {
        if self.hunger < 25.0 {
            return Mood::Hungry;
        }
        if self.energy < 20.0 {
            return Mood::Tired;
        }
        if self.cleanliness < 25.0 {
            return Mood::Dirty;
        }
        if self.happiness < 30.0 {
            return Mood::Sad;
        }
        if self.happiness > 70.0 && self.hunger > 60.0 && self.energy > 60.0 {
            return Mood::Happy;
        }
        Mood::Content
    }

    #[allow(dead_code)]
    pub fn lifecycle_stage(&self, now: f64) -> &'static str {
        let days = (now - self.birth_date) / 86400.0;
        if days < 1.0 {
            "egg"
        } else if days < 4.0 {
            "baby"
        } else if days < 10.0 {
            "teen"
        } else {
            "adult"
        }
    }
}

/// Loads/saves `PetState` as JSON in `%APPDATA%\ClaudePet\state.json`, atomically.
/// Mirrors `PetStateStore` in the Swift source.
pub struct PetStateStore;

impl PetStateStore {
    fn file_path() -> Option<PathBuf> {
        let base = std::env::var_os("APPDATA")?;
        let dir = PathBuf::from(base).join("ClaudePet");
        let _ = std::fs::create_dir_all(&dir);
        Some(dir.join("state.json"))
    }

    pub fn load() -> PetState {
        let Some(path) = Self::file_path() else {
            return PetState::default();
        };
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => PetState::default(),
        }
    }

    pub fn save(state: &PetState) {
        let Some(path) = Self::file_path() else { return };
        let Ok(json) = serde_json::to_vec_pretty(state) else {
            return;
        };
        // Write to a sibling temp file then rename, so a crash mid-write can
        // never leave a half-written state.json.
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, &json).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ports Tests/ClaudePetTests/PetStateTests.swift.

    fn fixed_state(last_tick: f64) -> PetState {
        PetState {
            hunger: 80.0,
            energy: 100.0,
            happiness: 80.0,
            cleanliness: 100.0,
            birth_date: 0.0,
            last_tick,
            auto_update: true,
            skin: SkinId::default(),
            accessories: HashSet::new(),
        }
    }

    #[test]
    fn decay_over_one_hour() {
        let mut state = fixed_state(0.0);
        state.tick(3600.0);
        assert!((state.hunger - 77.0).abs() < 0.01);
        assert!((state.energy - 98.0).abs() < 0.01);
        assert!((state.happiness - 78.5).abs() < 0.01);
        assert!((state.cleanliness - 99.0).abs() < 0.01);
    }

    #[test]
    fn stats_never_go_below_zero() {
        let mut state = fixed_state(0.0);
        state.tick(100.0 * 86400.0);
        assert!(state.hunger >= 0.0);
        assert!(state.energy >= 0.0);
        assert!(state.happiness >= 0.0);
        assert!(state.cleanliness >= 0.0);
    }

    #[test]
    fn stats_never_exceed_one_hundred() {
        let mut state = PetState::default();
        for _ in 0..5 {
            state.feed();
        }
        for _ in 0..3 {
            state.play();
        }
        for _ in 0..3 {
            state.pet();
        }
        assert!(state.hunger <= 100.0);
        assert!(state.happiness <= 100.0);
    }

    #[test]
    fn long_absence_is_clamped_not_zeroed() {
        let mut under_cap = fixed_state(0.0);
        under_cap.tick(11.0 * 3600.0); // under 12h cap
        assert!(under_cap.hunger > 0.0);

        let mut over_cap = fixed_state(0.0);
        over_cap.tick(50.0 * 3600.0); // well over 12h cap
        assert!((over_cap.hunger - under_cap.hunger).abs() < 5.0);
    }

    #[test]
    fn negative_clock_delta_is_ignored() {
        let mut state = fixed_state(10_000.0);
        let before = state.hunger;
        state.tick(10_000.0 - 3600.0);
        assert!((state.hunger - before).abs() < 0.001);
    }

    #[test]
    fn mood_priority_hunger_over_tiredness() {
        let mut state = PetState::default();
        state.hunger = 10.0;
        state.energy = 10.0;
        assert_eq!(state.mood(), Mood::Hungry);
    }

    #[test]
    fn skin_and_accessories_round_trip_through_json() {
        let mut state = PetState::default();
        state.skin = SkinId::Clown;
        state.accessories.insert(AccessoryId::TopHat);
        state.accessories.insert(AccessoryId::Glasses);

        let json = serde_json::to_string(&state).unwrap();
        let decoded: PetState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.skin, SkinId::Clown);
        assert_eq!(decoded.accessories.len(), 2);
    }

    #[test]
    fn state_json_missing_skin_fields_decodes_as_classic_with_no_accessories() {
        // Shaped like a state.json saved before skins existed.
        let json = r#"{"hunger":80.0,"energy":100.0,"happiness":80.0,"cleanliness":100.0,"birth_date":0.0,"last_tick":0.0,"auto_update":true}"#;
        let decoded: PetState = serde_json::from_str(json).unwrap();
        assert_eq!(decoded.skin, SkinId::Classic);
        assert!(decoded.accessories.is_empty());
    }
}
