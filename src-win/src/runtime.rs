//! Owns the pet's clock: state, brain, physics, couriers, messaging, and the
//! per-frame sprite selection. The Win32 shell (`main.rs`) pumps events and
//! calls `tick`; everything else is driven from here.
//! Mirrors `Sources/ClaudePet/Runtime.swift`.

use crate::distraction::DistractionDetector;
use crate::geometry;
use crate::ledges::{self, Ledge};
use crate::net::{Kind, PeerTransport, PetMessage};
use crate::pet::brain::{AnimState, Brain};
use crate::pet::courier::{Courier, Phase, HANDOFF_DURATION};
use crate::pet::dialogue::Dialogue;
use crate::pet::pet_state::{now_secs, PetState, PetStateStore};
use crate::pet::sprites::{CLIPS, GRID_SIZE};
use std::collections::VecDeque;

pub const ZOOM: i32 = 5;
pub const SPRITE_PX: i32 = GRID_SIZE as i32 * ZOOM; // 80
const GRAVITY_ACCEL: f64 = 1400.0; // pt/s^2  (toward +Y / down)
const TERMINAL_FALL_SPEED: f64 = 1600.0; // pt/s
const NAP_DURATION: f64 = 5.0 * 60.0;
const LEDGE_REFRESH_INTERVAL: f64 = 0.5;
const SAVE_INTERVAL: f64 = 20.0;
const ANGRY_BUBBLE_INTERVAL: f64 = 3.5;

/// The thin orange dock bar. Click it to send the pet "into" the bar (hidden);
/// click again and it tumbles back onto the screen.
pub const BAR_W: i32 = 4;
pub const BAR_H: i32 = 90;
const BAR_HIT_PAD: i32 = 8; // the bar is tiny - give clicks a bigger target
const DOCK_IN_SECS: f64 = 0.30; // "jumps in" glide toward the bar
const UNDOCK_KICK: f64 = 240.0; // px/s sideways shove on "tumbles out"

/// What `main` needs to blit a sprite this frame.
pub struct FrameSprite {
    pub x: i32,
    pub y: i32,
    pub anim: AnimState,
    pub frame: usize,
    pub facing_right: bool,
}

pub struct Runtime {
    pub state: PetState,
    brain: Brain,
    dialogue: Dialogue,
    transport: Box<dyn PeerTransport>,
    distraction: DistractionDetector,
    local_name: String,
    own_hwnd: isize,

    pet_x: f64,
    pet_y: f64,
    frame_index: usize,
    frame_elapsed: f64,
    last_tick: f64,
    last_save: f64,
    persist_enabled: bool,

    ledges: Vec<Ledge>,
    last_ledge_refresh: f64,
    fall_velocity: f64,
    sleep_start: Option<f64>,

    is_drag_active: bool,
    drag_off_x: f64,
    drag_off_y: f64,

    // orange dock bar
    bar_x: f64,
    bar_y: f64,
    docked: bool,
    dock_in: Option<f64>, // Some(elapsed) while gliding into the bar
    dock_in_from: (f64, f64),
    undock_kick: f64, // decaying sideways velocity after tumbling out
    bar_drag: bool,
    bar_off_x: f64,
    bar_off_y: f64,

    // pet-to-pet messaging
    outbound: Option<Courier>,
    outbound_msg_id: Option<String>,
    outbound_ack: bool,
    inbound: Option<Courier>,
    inbound_msg: Option<PetMessage>,
    inbound_bubble_shown: bool,
    visitor_x: f64,
    visitor_frame_index: usize,
    visitor_frame_elapsed: f64,
    visitor_anim: AnimState,
    visitor_facing_right: bool,
    pending: VecDeque<PetMessage>,

    bubble_text: Option<String>,
    bubble_until: f64,
    last_angry_bubble: f64,

    last_distraction_check: f64,
    distraction_interval: f64,

    known_peers: Vec<String>,
}

impl Runtime {
    pub fn new(mut transport: Box<dyn PeerTransport>, own_hwnd: isize) -> Self {
        let state = PetStateStore::load();
        let local_name = transport.local_name();
        transport.start();

        let area = geometry::primary_work_area();
        let now = now_secs();
        let mut rt = Runtime {
            state,
            brain: Brain::new(),
            dialogue: Dialogue::new(),
            transport,
            distraction: DistractionDetector::new(),
            local_name,
            own_hwnd,
            pet_x: (area.left + area.width() / 2.0 - SPRITE_PX as f64 / 2.0).round(),
            pet_y: (area.bottom - SPRITE_PX as f64).round(),
            frame_index: 0,
            frame_elapsed: 0.0,
            last_tick: now,
            last_save: now,
            persist_enabled: true,
            ledges: Vec::new(),
            last_ledge_refresh: 0.0,
            fall_velocity: 0.0,
            sleep_start: None,
            is_drag_active: false,
            drag_off_x: 0.0,
            drag_off_y: 0.0,
            bar_x: 6.0,
            bar_y: 44.0,
            docked: false,
            dock_in: None,
            dock_in_from: (0.0, 0.0),
            undock_kick: 0.0,
            bar_drag: false,
            bar_off_x: 0.0,
            bar_off_y: 0.0,
            outbound: None,
            outbound_msg_id: None,
            outbound_ack: false,
            inbound: None,
            inbound_msg: None,
            inbound_bubble_shown: false,
            visitor_x: 0.0,
            visitor_frame_index: 0,
            visitor_frame_elapsed: 0.0,
            visitor_anim: AnimState::Walk,
            visitor_facing_right: true,
            pending: VecDeque::new(),
            bubble_text: None,
            bubble_until: 0.0,
            last_angry_bubble: f64::NEG_INFINITY,
            last_distraction_check: f64::NEG_INFINITY,
            distraction_interval: 2.5,
            known_peers: Vec::new(),
        };
        rt.refresh_ledges();
        rt
    }

    // ---- driven by main -------------------------------------------------

    pub fn tick(&mut self) {
        self.tick_at(now_secs());
    }

    /// The tick body with an injected clock, so the messaging/physics dance can
    /// be driven deterministically from tests.
    pub fn tick_at(&mut self, now: f64) {
        let dt = (now - self.last_tick).max(0.0);
        self.last_tick = now;

        self.state.tick(now);

        // Cap a nap at a fixed real-time length rather than leaving it to energy
        // recovery alone.
        if self.brain.anim() == AnimState::Sleep {
            let start = *self.sleep_start.get_or_insert(now);
            if now - start >= NAP_DURATION {
                self.state.energy = self.state.energy.max(65.0);
                self.brain.wake(now);
                self.sleep_start = None;
            }
        } else {
            self.sleep_start = None;
        }
        if self.state.energy > 60.0 {
            self.brain.wake(now);
        }

        while let Some((msg, from)) = self.transport.try_recv() {
            self.handle_received(msg, from, now);
        }

        if now - self.last_distraction_check >= self.distraction_interval {
            let distracted = self.distraction.currently_distracted();
            self.brain.set_distracted(distracted);
            self.distraction_interval = if distracted { 1.0 } else { 2.5 };
            self.last_distraction_check = now;
        }

        let delivery_busy = self.tick_messaging(now, dt);

        self.advance_dock(dt);

        if !self.is_drag_active && !delivery_busy && !self.docked && self.dock_in.is_none() {
            self.apply_gravity(dt);
            if self.undock_kick.abs() > 1.0 {
                self.move_x(self.undock_kick * dt);
                self.undock_kick *= 0.5_f64.powf(dt / 0.30); // ~0.30s half-life
                if self.undock_kick.abs() <= 1.0 {
                    self.undock_kick = 0.0;
                }
            }
            let dx = self.brain.tick(now, self.state.mood());
            if dx != 0.0 {
                self.move_x(dx);
            }
        }

        if now - self.last_ledge_refresh >= LEDGE_REFRESH_INTERVAL {
            self.refresh_ledges();
            self.last_ledge_refresh = now;
        }

        self.advance_frame(dt);

        if self.brain.is_distracted() && now - self.last_angry_bubble > ANGRY_BUBBLE_INTERVAL {
            self.last_angry_bubble = now;
            let line = self.dialogue.angry_line().to_string();
            self.set_bubble(line, 3.0);
        }

        if self.bubble_until < now {
            self.bubble_text = None;
        }

        if now - self.last_save > SAVE_INTERVAL {
            self.persist_now();
        }

        self.known_peers = self.transport.peer_names();
    }

    /// True while walking/angry/falling/dancing or a courier is active - drives
    /// the 30fps vs 8fps timer cadence.
    pub fn is_fast_motion(&self) -> bool {
        matches!(
            self.brain.anim(),
            AnimState::Walk | AnimState::Angry | AnimState::Fall | AnimState::Dance
        ) || self.outbound.is_some()
            || self.inbound.is_some()
            || self.dock_in.is_some()
            || self.undock_kick.abs() > 1.0
    }

    fn advance_dock(&mut self, dt: f64) {
        let Some(elapsed) = self.dock_in else { return };
        let e = elapsed + dt;
        let f = (e / DOCK_IN_SECS).min(1.0);
        let ease = f * f; // ease-in: the pet accelerates as it "jumps in"
        let (tx, ty) = self.bar_pet_origin();
        self.pet_x = self.dock_in_from.0 + (tx - self.dock_in_from.0) * ease;
        self.pet_y = self.dock_in_from.1 + (ty - self.dock_in_from.1) * ease;
        if f >= 1.0 {
            self.dock_in = None;
            self.docked = true;
        } else {
            self.dock_in = Some(e);
        }
    }

    /// Pet top-left such that its centre sits on the bar's centre.
    fn bar_pet_origin(&self) -> (f64, f64) {
        (
            self.bar_x + BAR_W as f64 / 2.0 - SPRITE_PX as f64 / 2.0,
            self.bar_y + BAR_H as f64 / 2.0 - SPRITE_PX as f64 / 2.0,
        )
    }

    pub fn peer_names(&self) -> &[String] {
        &self.known_peers
    }

    // ---- input from main ----------------------------------------------

    pub fn cursor_over_pet(&self, cx: i32, cy: i32) -> bool {
        let Some(s) = self.pet_sprite() else { return false };
        if cx < s.x || cy < s.y || cx >= s.x + SPRITE_PX || cy >= s.y + SPRITE_PX {
            return false;
        }
        // Per-pixel: is this grid cell opaque?
        let Some(clip) = CLIPS.get(&s.anim) else { return false };
        let frame = &clip.frames[s.frame.min(clip.frames.len() - 1)];
        let mut col = ((cx - s.x) / ZOOM) as usize;
        let row = ((cy - s.y) / ZOOM) as usize;
        if s.facing_right {
            // nothing
        } else {
            col = GRID_SIZE - 1 - col;
        }
        frame
            .get(row)
            .and_then(|r| r.get(col))
            .map(|&v| v != 0)
            .unwrap_or(false)
    }

    pub fn begin_drag(&mut self, cx: i32, cy: i32) {
        self.is_drag_active = true;
        self.drag_off_x = cx as f64 - self.pet_x;
        self.drag_off_y = cy as f64 - self.pet_y;
        self.brain.begin_drag();
    }

    pub fn drag_to(&mut self, cx: i32, cy: i32) {
        if !self.is_drag_active {
            return;
        }
        self.pet_x = cx as f64 - self.drag_off_x;
        self.pet_y = cy as f64 - self.drag_off_y;
    }

    pub fn end_drag(&mut self) {
        self.is_drag_active = false;
        self.brain.end_drag();
        // gravity/landing on the next ticks settles it onto a ledge naturally.
    }

    pub fn is_dragging(&self) -> bool {
        self.is_drag_active
    }

    // ---- orange dock bar --------------------------------------------

    pub fn bar_rect(&self) -> (i32, i32, i32, i32) {
        (self.bar_x.round() as i32, self.bar_y.round() as i32, BAR_W, BAR_H)
    }

    pub fn cursor_over_bar(&self, cx: i32, cy: i32) -> bool {
        let (x, y, w, h) = self.bar_rect();
        cx >= x - BAR_HIT_PAD
            && cx <= x + w + BAR_HIT_PAD
            && cy >= y - BAR_HIT_PAD
            && cy <= y + h + BAR_HIT_PAD
    }

    /// Click the bar: send the pet gliding into it (then hidden); click again and
    /// it tumbles back out onto the screen.
    pub fn toggle_dock(&mut self) {
        if self.docked || self.dock_in.is_some() {
            self.undock();
        } else {
            self.dock_in = Some(0.0);
            self.dock_in_from = (self.pet_x, self.pet_y);
        }
    }

    fn undock(&mut self) {
        self.docked = false;
        self.dock_in = None;
        let (tx, ty) = self.bar_pet_origin();
        self.pet_x = tx;
        self.pet_y = ty;
        self.fall_velocity = 30.0;
        let (sw, _) = geometry::primary_screen_size();
        self.undock_kick = if self.bar_x + BAR_W as f64 / 2.0 <= sw as f64 / 2.0 {
            UNDOCK_KICK
        } else {
            -UNDOCK_KICK
        };
        self.brain.set_falling(true);
    }

    pub fn begin_bar_drag(&mut self, cx: i32, cy: i32) {
        self.bar_drag = true;
        self.bar_off_x = cx as f64 - self.bar_x;
        self.bar_off_y = cy as f64 - self.bar_y;
    }

    pub fn bar_drag_to(&mut self, cx: i32, cy: i32) {
        if !self.bar_drag {
            return;
        }
        let (sw, sh) = geometry::primary_screen_size();
        self.bar_x = (cx as f64 - self.bar_off_x).clamp(0.0, (sw - BAR_W) as f64);
        self.bar_y = (cy as f64 - self.bar_off_y).clamp(0.0, (sh - BAR_H) as f64);
    }

    pub fn end_bar_drag(&mut self) {
        self.bar_drag = false;
        // It's an edge widget - snap to whichever vertical screen edge is nearer.
        let (sw, _) = geometry::primary_screen_size();
        self.bar_x = if self.bar_x + BAR_W as f64 / 2.0 < sw as f64 / 2.0 {
            6.0
        } else {
            (sw - BAR_W - 6) as f64
        };
    }

    pub fn on_pet_click(&mut self) {
        if self.is_drag_active {
            return;
        }
        self.state.pet();
        self.show_mood_bubble();
    }

    pub fn feed(&mut self) {
        self.state.feed();
        self.brain.celebrate(now_secs());
        let line = self.dialogue.celebration_line().to_string();
        self.set_bubble(line, 3.0);
        self.persist_now();
    }

    pub fn play(&mut self) {
        self.state.play();
        self.set_bubble("leveraging some blue-sky thinking".into(), 3.0);
        self.persist_now();
    }

    pub fn clean(&mut self) {
        self.state.clean();
        self.set_bubble("optimizing my core competencies".into(), 3.0);
        self.persist_now();
    }

    pub fn display_screen_changed(&mut self) {
        let (x, y) = geometry::clamp_origin(self.pet_x, self.pet_y, SPRITE_PX as f64, SPRITE_PX as f64);
        self.pet_x = x;
        self.pet_y = y;
        self.refresh_ledges();
    }

    /// Names of currently-known peers, owned (so the caller can drop the runtime
    /// borrow before opening a modal composer that pumps messages).
    pub fn peer_names_owned(&self) -> Vec<String> {
        self.transport.peer_names()
    }

    // ---- messaging ---------------------------------------------------

    pub fn send_message(&mut self, text: &str, peer: &str) {
        self.send_message_at(text, peer, now_secs());
    }

    pub fn send_message_at(&mut self, text: &str, peer: &str, now: f64) {
        if self.outbound.is_some() {
            return;
        }
        let center_x = self.pet_x + SPRITE_PX as f64 / 2.0;
        let area = geometry::work_area_containing(center_x, self.pet_y + SPRITE_PX as f64 / 2.0);
        let home_x = self.pet_x;
        let left_gap = home_x - area.left;
        let right_gap = area.right - home_x - SPRITE_PX as f64;
        let edge = if left_gap < right_gap {
            crate::net::Edge::Left
        } else {
            crate::net::Edge::Right
        };
        let off_screen_x = match edge {
            crate::net::Edge::Right => area.right + SPRITE_PX as f64,
            crate::net::Edge::Left => area.left - SPRITE_PX as f64,
        };

        let message = PetMessage::deliver(text.to_string(), self.local_name.clone(), edge);
        self.outbound_msg_id = Some(message.id.clone());
        self.outbound_ack = false;
        self.outbound = Some(Courier::outbound(home_x, home_x, off_screen_x, edge, now));
        self.brain.set_falling(false);
        let line = self.dialogue.depart_line().to_string();
        self.set_bubble(line, 3.0);
        self.transport.send(&message, peer);
    }

    fn handle_received(&mut self, message: PetMessage, _from: String, now: f64) {
        match message.kind {
            Kind::Ack => {
                if self.outbound_msg_id.as_deref() == Some(message.id.as_str()) {
                    self.outbound_ack = true;
                    if let Some(c) = &mut self.outbound {
                        c.received_ack();
                    }
                }
            }
            Kind::Deliver => {
                // A letter's arriving - pop back out of the bar to receive it.
                if self.docked || self.dock_in.is_some() {
                    self.undock();
                }
                self.pending.push_back(message);
                self.start_next_delivery_if_idle(now);
            }
        }
    }

    fn start_next_delivery_if_idle(&mut self, now: f64) {
        if self.inbound.is_some() {
            return;
        }
        let Some(message) = self.pending.pop_front() else {
            return;
        };

        let center_x = self.pet_x + SPRITE_PX as f64 / 2.0;
        let area = geometry::work_area_containing(center_x, self.pet_y + SPRITE_PX as f64 / 2.0);
        let entry_edge = message.exit_edge.opposite();
        let off_screen_x = match entry_edge {
            crate::net::Edge::Right => area.right + SPRITE_PX as f64,
            crate::net::Edge::Left => area.left - SPRITE_PX as f64,
        };
        let handoff_offset = 60.0;
        let handoff_x = match entry_edge {
            crate::net::Edge::Right => self.pet_x + handoff_offset,
            crate::net::Edge::Left => self.pet_x - handoff_offset,
        };

        self.visitor_x = off_screen_x;
        self.visitor_frame_index = 0;
        self.visitor_frame_elapsed = 0.0;
        self.inbound_bubble_shown = false;
        self.inbound = Some(Courier::inbound(off_screen_x, handoff_x, entry_edge, now));
        self.inbound_msg = Some(message);
    }

    /// Returns whether the resident pet's own movement should be suppressed this
    /// tick because it's busy delivering.
    fn tick_messaging(&mut self, now: f64, dt: f64) -> bool {
        let mut suppress = false;

        if let Some(courier) = &mut self.outbound {
            let was_away = courier.is_away();
            courier.tick(now);
            match courier.phase() {
                Phase::Departing | Phase::Returning => {
                    if was_away && !self.outbound_ack {
                        let line = self.dialogue.delivery_failed_line().to_string();
                        self.bubble_text = Some(line);
                        self.bubble_until = now + 3.0;
                    }
                    self.pet_x = courier.x();
                    suppress = true;
                }
                Phase::Away => {
                    suppress = true;
                }
                Phase::Done => {
                    self.outbound = None;
                    self.outbound_msg_id = None;
                    self.brain.set_falling(false);
                }
                _ => {}
            }
        }

        let mut finished_inbound = false;
        if let Some(courier) = &mut self.inbound {
            courier.tick(now);
            self.visitor_x = courier.x();
            self.visitor_anim = courier.anim();
            self.visitor_facing_right = courier.facing_right();
            advance_clip(
                self.visitor_anim,
                dt,
                &mut self.visitor_frame_index,
                &mut self.visitor_frame_elapsed,
            );
            match courier.phase() {
                Phase::Done => {
                    if let Some(m) = &self.inbound_msg {
                        self.transport.send(&m.make_ack(), &m.sender_name);
                    }
                    finished_inbound = true;
                }
                Phase::Handing if !self.inbound_bubble_shown => {
                    self.inbound_bubble_shown = true;
                    if let Some(m) = &self.inbound_msg {
                        self.bubble_text = Some(m.text.clone());
                        self.bubble_until = now + HANDOFF_DURATION;
                    }
                }
                _ => {}
            }
        }
        if finished_inbound {
            self.inbound = None;
            self.inbound_msg = None;
            self.start_next_delivery_if_idle(now);
        }

        suppress
    }

    // ---- physics / motion -------------------------------------------

    fn move_x(&mut self, dx: f64) {
        self.pet_x += dx;
        let (x, y) =
            geometry::clamp_origin(self.pet_x, self.pet_y, SPRITE_PX as f64, SPRITE_PX as f64);
        self.pet_x = x;
        self.pet_y = y;
    }

    fn apply_gravity(&mut self, dt: f64) {
        let foot_x = self.pet_x + SPRITE_PX as f64 / 2.0;
        let foot_y = self.pet_y + SPRITE_PX as f64;

        // Already standing on something?
        if let Some(l) = ledges::ledge_below(foot_x, foot_y - 1.0, &self.ledges) {
            if (l.y - foot_y).abs() < 1.0 {
                if self.fall_velocity != 0.0 {
                    self.fall_velocity = 0.0;
                    self.brain.set_falling(false);
                }
                return;
            }
        }

        let Some(target) = ledges::ledge_below(foot_x, foot_y, &self.ledges) else {
            return; // nothing below - hold in place
        };

        self.brain.set_falling(true);
        self.fall_velocity = (self.fall_velocity + dt * GRAVITY_ACCEL).min(TERMINAL_FALL_SPEED);
        let proposed_foot_y = foot_y + self.fall_velocity * dt;
        if proposed_foot_y >= target.y {
            self.pet_y = target.y - SPRITE_PX as f64;
            self.fall_velocity = 0.0;
            self.brain.set_falling(false);
        } else {
            self.pet_y = proposed_foot_y - SPRITE_PX as f64;
        }
    }

    fn refresh_ledges(&mut self) {
        self.ledges = ledges::current_ledges(self.own_hwnd, 24.0);
    }

    // ---- rendering data --------------------------------------------

    fn resident_anim(&self) -> AnimState {
        match &self.outbound {
            Some(c) if c.phase() != Phase::Away => c.anim(),
            _ => self.brain.anim(),
        }
    }

    fn resident_facing_right(&self) -> bool {
        match &self.outbound {
            Some(c) if c.phase() != Phase::Away => c.facing_right(),
            _ => self.brain.facing_right(),
        }
    }

    fn advance_frame(&mut self, dt: f64) {
        let anim = self.resident_anim();
        advance_clip(anim, dt, &mut self.frame_index, &mut self.frame_elapsed);
    }

    /// The resident pet sprite for this frame, or `None` while it's off-screen
    /// delivering or fully docked in the orange bar.
    pub fn pet_sprite(&self) -> Option<FrameSprite> {
        if self.docked {
            return None;
        }
        if self.outbound.as_ref().map_or(false, |c| c.is_away()) {
            return None;
        }
        let anim = self.resident_anim();
        let len = CLIPS.get(&anim).map(|c| c.frames.len()).unwrap_or(1);
        Some(FrameSprite {
            x: self.pet_x.round() as i32,
            y: self.pet_y.round() as i32,
            anim,
            frame: self.frame_index.min(len.saturating_sub(1)),
            facing_right: self.resident_facing_right(),
        })
    }

    pub fn visitor_sprite(&self) -> Option<FrameSprite> {
        self.inbound.as_ref()?;
        let len = CLIPS.get(&self.visitor_anim).map(|c| c.frames.len()).unwrap_or(1);
        Some(FrameSprite {
            x: self.visitor_x.round() as i32,
            y: self.pet_y.round() as i32,
            anim: self.visitor_anim,
            frame: self.visitor_frame_index.min(len.saturating_sub(1)),
            facing_right: self.visitor_facing_right,
        })
    }

    /// `(text, center_x, bottom_y)` for the speech bubble, if one is showing.
    pub fn bubble(&self) -> Option<(&str, i32, i32)> {
        let text = self.bubble_text.as_deref()?;
        let center_x = (self.pet_x + SPRITE_PX as f64 / 2.0).round() as i32;
        let bottom_y = (self.pet_y - 6.0).round() as i32;
        Some((text, center_x, bottom_y))
    }

    // ---- helpers --------------------------------------------------

    fn set_bubble(&mut self, text: String, dur: f64) {
        self.bubble_text = Some(text);
        self.bubble_until = now_secs() + dur;
    }

    fn show_mood_bubble(&mut self) {
        let line = self.dialogue.line(self.state.mood()).to_string();
        self.set_bubble(line, 3.0);
    }

    fn persist_now(&mut self) {
        self.last_save = now_secs();
        if self.persist_enabled {
            PetStateStore::save(&self.state);
        }
    }

    /// Test hook: stop this runtime from touching `%APPDATA%\ClaudePet\state.json`.
    #[cfg(test)]
    fn disable_persistence(&mut self) {
        self.persist_enabled = false;
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        if self.persist_enabled {
            PetStateStore::save(&self.state);
        }
    }
}

/// Advance a looping clip's frame counter by `dt`.
fn advance_clip(anim: AnimState, dt: f64, index: &mut usize, elapsed: &mut f64) {
    let Some(clip) = CLIPS.get(&anim) else { return };
    *elapsed += dt;
    if *elapsed >= clip.frame_duration {
        *elapsed = 0.0;
        *index = (*index + 1) % clip.frames.len().max(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{Kind, PeerTransport, PetMessage};
    use crate::pet::dialogue::is_delivery_failed_line;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    /// In-process transport whose queues the test keeps a handle to.
    #[derive(Clone)]
    struct FakeTransport {
        sent: Arc<Mutex<Vec<(PetMessage, String)>>>,
        inbox: Arc<Mutex<VecDeque<(PetMessage, String)>>>,
        peers: Arc<Vec<String>>,
    }

    impl FakeTransport {
        fn with_peers(peers: &[&str]) -> Self {
            FakeTransport {
                sent: Arc::new(Mutex::new(Vec::new())),
                inbox: Arc::new(Mutex::new(VecDeque::new())),
                peers: Arc::new(peers.iter().map(|s| s.to_string()).collect()),
            }
        }
    }

    impl PeerTransport for FakeTransport {
        fn start(&mut self) {}
        fn local_name(&self) -> String {
            "TestPet".into()
        }
        fn peer_names(&self) -> Vec<String> {
            (*self.peers).clone()
        }
        fn send(&self, message: &PetMessage, to_peer: &str) {
            self.sent.lock().unwrap().push((message.clone(), to_peer.to_string()));
        }
        fn try_recv(&self) -> Option<(PetMessage, String)> {
            self.inbox.lock().unwrap().pop_front()
        }
    }

    impl Runtime {
        fn t_outbound_phase(&self) -> Option<Phase> {
            self.outbound.as_ref().map(|c| c.phase())
        }
        fn t_ack(&self) -> bool {
            self.outbound_ack
        }
    }

    fn new_rt(fake: &FakeTransport) -> Runtime {
        let mut rt = Runtime::new(Box::new(fake.clone()), 0);
        rt.disable_persistence();
        rt
    }

    /// A big-enough time step (at 90 pt/s) to walk the courier fully off any
    /// single monitor, so `send`/tick sequencing is resolution-independent.
    const FAR: f64 = 30.0;

    #[test]
    fn ack_while_away_returns_home_without_the_failure_bubble() {
        let fake = FakeTransport::with_peers(&["PeerX"]);
        let mut rt = new_rt(&fake);

        rt.send_message_at("ship it", "PeerX", 1000.0);
        let sent = fake.sent.lock().unwrap().clone();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0.kind, Kind::Deliver);
        assert_eq!(sent[0].1, "PeerX");
        let delivered = sent[0].0.clone();

        rt.tick_at(1000.0 + FAR); // courier walks off-screen -> Away
        assert_eq!(rt.t_outbound_phase(), Some(Phase::Away));

        // The peer acks.
        fake.inbox
            .lock()
            .unwrap()
            .push_back((delivered.make_ack(), "PeerX".into()));
        rt.tick_at(1000.0 + FAR + 0.5);

        assert!(rt.t_ack(), "ack should have been recorded");
        assert!(matches!(
            rt.t_outbound_phase(),
            Some(Phase::Returning) | Some(Phase::Done) | None
        ));
        let bubble = rt.bubble().map(|b| b.0.to_string());
        assert!(
            bubble.as_deref().map_or(true, |b| !is_delivery_failed_line(b)),
            "ack path must not show a delivery-failed line, got {bubble:?}"
        );
    }

    #[test]
    fn timeout_without_ack_shows_the_failure_bubble() {
        let fake = FakeTransport::with_peers(&["PeerX"]);
        let mut rt = new_rt(&fake);

        rt.send_message_at("hello?", "PeerX", 1000.0);
        rt.tick_at(1000.0 + FAR); // -> Away, deadline = (1000+FAR) + 10
        assert_eq!(rt.t_outbound_phase(), Some(Phase::Away));

        rt.tick_at(1000.0 + FAR + 5.0); // still waiting
        assert_eq!(rt.t_outbound_phase(), Some(Phase::Away));

        rt.tick_at(1000.0 + FAR + 11.0); // past the 10s wait, no ack
        assert!(!rt.t_ack());
        let bubble = rt.bubble().map(|b| b.0.to_string()).expect("a bubble should be showing");
        assert!(
            is_delivery_failed_line(&bubble),
            "timeout path should show a delivery-failed line, got {bubble:?}"
        );
    }

    #[test]
    fn inbound_delivery_walks_a_visitor_and_acks_back() {
        let fake = FakeTransport::with_peers(&["Sender"]);
        let mut rt = new_rt(&fake);

        let msg = PetMessage::deliver("great progress".into(), "Sender".into(), crate::net::Edge::Right);
        fake.inbox.lock().unwrap().push_back((msg, "Sender".into()));

        rt.tick_at(2000.0); // spawns the inbound courier + visitor
        assert!(rt.visitor_sprite().is_some(), "a visitor should be walking in");

        // Walk it through arrival -> handoff -> departure.
        rt.tick_at(2000.0 + FAR);
        rt.tick_at(2000.0 + FAR + 3.0);
        rt.tick_at(2000.0 + 2.0 * FAR + 3.0);

        let acks: Vec<_> = fake
            .sent
            .lock()
            .unwrap()
            .iter()
            .filter(|(m, _)| m.kind == Kind::Ack)
            .cloned()
            .collect();
        assert_eq!(acks.len(), 1, "exactly one ack should have been sent back");
        assert_eq!(acks[0].1, "Sender");
        assert!(rt.visitor_sprite().is_none(), "visitor should be gone once done");
    }
}
