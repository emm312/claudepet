//! Owns the pet's clock: state, brain, physics, couriers, messaging, and the
//! per-frame sprite selection. The Win32 shell (`main.rs`) pumps events and
//! calls `tick`; everything else is driven from here.
//! Mirrors `Sources/ClaudePet/Runtime.swift`.

use crate::distraction::DistractionDetector;
use crate::geometry;
use crate::ledges::{self, Ledge};
use crate::net::{Kind, PeerTransport, PetMessage};
use crate::pet::brain::{AnimState, Brain};
use crate::pet::courier::{Courier, Phase};
use crate::pet::dialogue::Dialogue;
use crate::pet::pet_state::{now_secs, PetState, PetStateStore};
use crate::pet::sprites::{AccessoryId, SkinId, CLIPS, GRID_SIZE};
use std::collections::{HashSet, VecDeque};

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
const EXPRESS_SPEED_MULT: f64 = 3.0; // courier speed on the horse - matches Courier.expressSpeedMultiplier in Swift

/// Nearest-neighbour zoom for the carried-mail sprite - a smaller factor than
/// the pet/horse `ZOOM` (5) so an 18x12 envelope doesn't render pet-sized.
pub const MAIL_ZOOM: i32 = 2;
/// How far the rider is lifted so it sits astride the horse's back rather than
/// overlapping it. Mirrors `HorseSprite.riderLift`.
pub const HORSE_RIDER_LIFT: i32 = 16;
/// Click padding around the little carried-mail rect - same idea as
/// `BAR_HIT_PAD`, the envelope is a small target.
const MAIL_HIT_PAD: i32 = 8;

/// What `main` needs to blit a sprite this frame.
pub struct FrameSprite {
    pub x: i32,
    pub y: i32,
    pub anim: AnimState,
    pub frame: usize,
    pub facing_right: bool,
    /// Carrying the mail (any courier leg).
    pub carry_mail: bool,
    /// Riding the horse (express delivery).
    pub on_horse: bool,
    /// Which of `HORSE_FRAMES` to draw - only meaningful when `on_horse`.
    pub horse_frame: usize,
    /// The actor's own chosen skin - `PetState::skin` for the resident, or the
    /// delivering `PetMessage`'s `sender_skin` for a visitor.
    pub skin: SkinId,
    pub accessories: Vec<AccessoryId>,
}

/// Picks a horse gallop frame from the wall clock, so `main::draw_actor`
/// doesn't need any dedicated animation state threaded through `Runtime`.
fn current_horse_frame() -> usize {
    ((now_secs() / crate::pet::sprites::HORSE_FRAME_DURATION) as u64 % 2) as usize
}

/// Screen rect `(x, y, w, h)` of the carried-mail sprite for `s`, in the same
/// place `main::draw_actor` blits it. Single source of truth so the click hit
/// test can't drift from the drawing.
pub fn mail_rect(s: &FrameSprite) -> (i32, i32, i32, i32) {
    let sprite_px = SPRITE_PX;
    let pet_y = if s.on_horse { s.y - HORSE_RIDER_LIFT } else { s.y };
    let mw = crate::pet::sprites::MAIL_GRID_COLS as i32 * MAIL_ZOOM;
    let mh = crate::pet::sprites::MAIL_GRID_ROWS as i32 * MAIL_ZOOM;
    let mx = if s.facing_right {
        s.x + sprite_px - mw - 4
    } else {
        s.x + 4
    };
    let my = pet_y + sprite_px - mh - 22;
    (mx, my, mw, mh)
}

/// One queued send: a fully-built message plus the peers it should go to.
struct OutboundJob {
    message: PetMessage,
    recipients: Vec<String>,
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
    /// Recipients that haven't acked the in-flight outbound message yet.
    outbound_pending: HashSet<String>,
    /// Recipients that have acked the in-flight outbound message.
    outbound_acked: HashSet<String>,
    outbound_express: bool,
    /// The datagram(s) for the in-flight trip are sent once, at `Away` entry -
    /// not at compose time - so a courier already `Departing` never races a
    /// same-tick ack. Cleared once sent.
    outbound_message: Option<PetMessage>,
    /// Sends queued while a previous trip is in flight. One `Courier` trip per
    /// message, regardless of recipient count, so a second `send_message`
    /// while away is queued rather than silently dropped.
    outbound_queue: VecDeque<OutboundJob>,
    inbound: Option<Courier>,
    inbound_msg: Option<PetMessage>,
    inbound_handed_off: bool,
    inbound_express: bool,
    /// Letters that have arrived but not been read yet. The resident pet keeps
    /// holding the mail sprite while this is non-empty; clicking it opens the
    /// front one in a letter window (see `letter.rs`). In-memory only.
    unread: VecDeque<PetMessage>,
    visitor_x: f64,
    visitor_y: f64,
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
    search_report_at: Option<f64>,
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
            outbound_pending: HashSet::new(),
            outbound_acked: HashSet::new(),
            outbound_express: false,
            outbound_message: None,
            outbound_queue: VecDeque::new(),
            inbound: None,
            inbound_msg: None,
            inbound_handed_off: false,
            inbound_express: false,
            unread: VecDeque::new(),
            visitor_x: 0.0,
            visitor_y: 0.0,
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
            search_report_at: None,
        };
        rt.refresh_ledges();

        // Dev aid: seed a delivered-but-unread letter so the letter window
        // (`letter.rs`) can be eyeballed with `cargo run` without a second box.
        // Debug builds only; ignored in the shipped release. It goes straight
        // into `unread` (not `inbound_msg`), so no ack is sent for it - expected.
        #[cfg(debug_assertions)]
        if std::env::var_os("CLAUDEPET_FAKE_LETTER").is_some() {
            rt.unread.push_back(PetMessage::deliver(
                "Ran the numbers you asked about \u{2014} the Q3 pipeline is up 18% and the \
                 board deck is ready for your review. Ping me when you want to walk through it."
                    .to_string(),
                "DeskMac".to_string(),
                crate::net::Edge::Right,
                false,
                SkinId::default(),
                Vec::new(),
            ));
        }

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

        // Report the result of a "Search for pets" once the scan window closes.
        if let Some(t) = self.search_report_at {
            if now >= t {
                self.search_report_at = None;
                let report = if self.known_peers.is_empty() {
                    "no pets nearby - is another one running on the LAN?".to_string()
                } else {
                    format!(
                        "found {} pet{}: {}",
                        self.known_peers.len(),
                        if self.known_peers.len() == 1 { "" } else { "s" },
                        self.known_peers.join(", ")
                    )
                };
                self.set_bubble(report, 4.5);
            }
        }
    }

    /// True while walking/angry/falling/dancing or a courier is active - drives
    /// the 30fps vs 8fps timer cadence.
    pub fn is_fast_motion(&self) -> bool {
        matches!(
            self.brain.anim(),
            AnimState::Walk | AnimState::Angry | AnimState::Fall | AnimState::Jump
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

    /// True when the cursor is over the carried-mail envelope of a pet that has
    /// an unread letter waiting - the click target that opens the letter window.
    /// Padded like the dock bar, and a plain rect (not per-pixel) since the
    /// envelope is small.
    pub fn cursor_over_mail(&self, cx: i32, cy: i32) -> bool {
        if self.unread.is_empty() {
            return false;
        }
        let Some(s) = self.pet_sprite() else { return false };
        if !s.carry_mail {
            return false;
        }
        let (mx, my, mw, mh) = mail_rect(&s);
        cx >= mx - MAIL_HIT_PAD
            && cx < mx + mw + MAIL_HIT_PAD
            && cy >= my - MAIL_HIT_PAD
            && cy < my + mh + MAIL_HIT_PAD
    }

    /// Is there a delivered letter waiting to be read?
    pub fn has_unread(&self) -> bool {
        !self.unread.is_empty()
    }

    /// A clone of the oldest unread letter, for the letter window to display.
    pub fn peek_unread(&self) -> Option<PetMessage> {
        self.unread.front().cloned()
    }

    /// Drop the oldest unread letter once it's been read.
    pub fn pop_unread(&mut self) -> Option<PetMessage> {
        self.unread.pop_front()
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
        self.brain.eat(now_secs());
        let line = self.dialogue.celebration_line().to_string();
        self.set_bubble(line, 3.0);
        self.persist_now();
    }

    pub fn play(&mut self) {
        self.state.play();
        self.brain.jump(now_secs());
        self.set_bubble("leveraging some blue-sky thinking".into(), 3.0);
        self.persist_now();
    }

    pub fn clean(&mut self) {
        self.state.clean();
        self.set_bubble("optimizing my core competencies".into(), 3.0);
        self.persist_now();
    }

    /// "Search for pets" - actively re-scan the LAN and report back in a bubble.
    pub fn search_for_pets(&mut self) {
        self.transport.rescan();
        self.set_bubble("scanning the org for nearby pets\u{2026}".into(), 3.5);
        self.search_report_at = Some(now_secs() + 3.0);
    }

    pub fn auto_update(&self) -> bool {
        self.state.auto_update
    }

    pub fn set_auto_update(&mut self, on: bool) {
        self.state.auto_update = on;
        self.persist_now();
    }

    pub fn save_now(&mut self) {
        self.persist_now();
    }

    // ---- skins/accessories --------------------------------------------

    pub fn skin(&self) -> SkinId {
        self.state.skin
    }

    pub fn accessories(&self) -> &HashSet<AccessoryId> {
        &self.state.accessories
    }

    /// Applies immediately (persists + the next frame renders it), mirroring
    /// `set_auto_update` - no separate "apply" step needed.
    pub fn set_skin(&mut self, id: SkinId) {
        self.state.skin = id;
        self.persist_now();
    }

    pub fn set_accessory(&mut self, id: AccessoryId, worn: bool) {
        if worn {
            self.state.accessories.insert(id);
        } else {
            self.state.accessories.remove(&id);
        }
        self.persist_now();
    }

    /// Stop the transport so peers see this pet drop off the LAN promptly
    /// (mDNS GOODBYE). Call right before the process exits - including the
    /// update-relaunch path - so a quick quit-then-relaunch doesn't leave a
    /// stale `_claudepet._udp` entry behind that hides the new instance.
    pub fn shutdown(&mut self) {
        self.transport.stop();
    }

    /// Shown just before the app swaps itself for a downloaded update.
    pub fn announce_update(&mut self, version: &str) {
        let v = version.trim_start_matches('v');
        self.set_bubble(format!("shipping v{v} - relaunching\u{2026}"), 12.0);
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

    pub fn send_message(&mut self, text: &str, peers: &[String], express: bool) {
        self.send_message_at(text, peers, express, now_secs());
    }

    /// Queues a message to one or more peers. One `Courier` trip carries it to
    /// every recipient; a send made while a previous trip is still in flight is
    /// queued rather than dropped - it starts as soon as the courier is free
    /// (`start_next_outbound_if_idle`, called from `tick_messaging`).
    pub fn send_message_at(&mut self, text: &str, peers: &[String], express: bool, now: f64) {
        if peers.is_empty() {
            return;
        }
        let edge = self.outbound_exit_edge();
        let message = PetMessage::deliver(
            text.to_string(),
            self.local_name.clone(),
            edge,
            express,
            self.state.skin,
            self.state.accessories.iter().copied().collect(),
        );
        self.outbound_queue.push_back(OutboundJob {
            message,
            recipients: peers.to_vec(),
        });
        self.start_next_outbound_if_idle(now);
    }

    fn outbound_exit_edge(&self) -> crate::net::Edge {
        let center_x = self.pet_x + SPRITE_PX as f64 / 2.0;
        let area = geometry::work_area_containing(center_x, self.pet_y + SPRITE_PX as f64 / 2.0);
        let home_x = self.pet_x;
        let left_gap = home_x - area.left;
        let right_gap = area.right - home_x - SPRITE_PX as f64;
        if left_gap < right_gap {
            crate::net::Edge::Left
        } else {
            crate::net::Edge::Right
        }
    }

    fn start_next_outbound_if_idle(&mut self, now: f64) {
        if self.outbound.is_some() {
            return;
        }
        let Some(job) = self.outbound_queue.pop_front() else {
            return;
        };
        let express = job.message.express;
        let edge = job.message.exit_edge;
        let center_x = self.pet_x + SPRITE_PX as f64 / 2.0;
        let area = geometry::work_area_containing(center_x, self.pet_y + SPRITE_PX as f64 / 2.0);
        let home_x = self.pet_x;
        let off_screen_x = match edge {
            crate::net::Edge::Right => area.right + SPRITE_PX as f64,
            crate::net::Edge::Left => area.left - SPRITE_PX as f64,
        };

        self.outbound_msg_id = Some(job.message.id.clone());
        self.outbound_pending = job.recipients.iter().cloned().collect();
        self.outbound_acked = HashSet::new();
        self.outbound_express = express;
        self.outbound_message = Some(job.message);
        let mult = if express { EXPRESS_SPEED_MULT } else { 1.0 };
        self.outbound = Some(Courier::outbound(home_x, home_x, off_screen_x, edge, now, mult));
        self.brain.set_falling(false);
        let line = if express {
            "saddling up - taking this one express".to_string()
        } else {
            self.dialogue.depart_line().to_string()
        };
        self.set_bubble(line, 3.0);
    }

    fn handle_received(&mut self, message: PetMessage, from: String, now: f64) {
        match message.kind {
            Kind::Ack => {
                if self.outbound_msg_id.as_deref() == Some(message.id.as_str()) {
                    self.outbound_pending.remove(&from);
                    self.outbound_acked.insert(from);
                    if let Some(c) = &mut self.outbound {
                        let wait = message.time_to_return.unwrap_or(crate::pet::courier::DEFAULT_WAIT);
                        c.received_ack(now, wait);
                    }
                }
            }
            Kind::Deliver => {
                // A letter's arriving - pop back out of the bar to receive it.
                if self.docked || self.dock_in.is_some() {
                    self.undock();
                }
                // Ack right away - the sender's timeout races real time, not the
                // visitor's walk-in/handoff/walk-out animation, so a slow or wide
                // screen no longer makes a delivered letter look "bounced". The
                // ack still tells the sender how long that animation will take
                // (computed on this screen) so its courier can wait for it
                // instead of turning around the instant the ack lands.
                let (_, off_screen_x, handoff_x, _) = self.inbound_geometry(&message);
                let mult = if message.express { EXPRESS_SPEED_MULT } else { 1.0 };
                let time_to_return =
                    crate::pet::courier::estimate_round_trip_duration((off_screen_x - handoff_x).abs(), mult);
                self.transport.send(&message.make_ack(&self.local_name, time_to_return), &from);
                self.pending.push_back(message);
                self.start_next_delivery_if_idle(now);
            }
        }
    }

    /// The entry edge, off-screen/handoff x positions, and work-area bottom an
    /// inbound courier for `message` would use, anchored on the resident
    /// pet's resting x (not the live `pet_x` - while an outbound trip is also
    /// in flight, `pet_x` is mid-transit and using it here would place the
    /// visitor at a bogus, possibly off-screen spot). Shared by
    /// `start_next_delivery_if_idle` and the ack's `time_to_return` estimate
    /// so both agree on the same trip.
    fn inbound_geometry(&self, message: &PetMessage) -> (crate::net::Edge, f64, f64, f64) {
        let base_x = self.outbound.as_ref().map_or(self.pet_x, |c| c.home_x());
        let center_x = base_x + SPRITE_PX as f64 / 2.0;
        let area = geometry::work_area_containing(center_x, self.pet_y + SPRITE_PX as f64 / 2.0);
        let entry_edge = message.exit_edge.opposite();
        let off_screen_x = match entry_edge {
            crate::net::Edge::Right => area.right + SPRITE_PX as f64,
            crate::net::Edge::Left => area.left - SPRITE_PX as f64,
        };
        let handoff_offset = 60.0;
        let handoff_x = match entry_edge {
            crate::net::Edge::Right => base_x + handoff_offset,
            crate::net::Edge::Left => base_x - handoff_offset,
        };
        (entry_edge, off_screen_x, handoff_x, area.bottom)
    }

    fn start_next_delivery_if_idle(&mut self, now: f64) {
        if self.inbound.is_some() {
            return;
        }
        let Some(message) = self.pending.pop_front() else {
            return;
        };

        let (entry_edge, off_screen_x, handoff_x, area_bottom) = self.inbound_geometry(&message);

        self.visitor_x = off_screen_x;
        self.visitor_y = area_bottom - SPRITE_PX as f64;
        self.visitor_frame_index = 0;
        self.visitor_frame_elapsed = 0.0;
        self.inbound_handed_off = false;
        self.inbound_express = message.express;
        let mult = if message.express { EXPRESS_SPEED_MULT } else { 1.0 };
        self.inbound = Some(Courier::inbound(off_screen_x, handoff_x, entry_edge, now, mult));
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
                Phase::Departing => {
                    self.pet_x = courier.x();
                    suppress = true;
                }
                Phase::Returning => {
                    if was_away && self.outbound_acked.is_empty() {
                        let line = self.dialogue.delivery_failed_line().to_string();
                        self.bubble_text = Some(line);
                        self.bubble_until = now + 3.0;
                    } else if was_away && !self.outbound_pending.is_empty() {
                        let missed: Vec<&str> = self.outbound_pending.iter().map(String::as_str).collect();
                        self.bubble_text = Some(format!("couldn't reach {}", missed.join(", ")));
                        self.bubble_until = now + 3.0;
                    }
                    self.pet_x = courier.x();
                    suppress = true;
                }
                Phase::Away => {
                    // Send exactly once, right as the courier arrives off-screen -
                    // not at compose time - so an ack that beats the walk-off
                    // animation is impossible and every recipient is queried at
                    // the same moment (the away-timeout is the same for all).
                    if let Some(message) = self.outbound_message.take() {
                        for peer in self.outbound_pending.iter().cloned().collect::<Vec<_>>() {
                            self.transport.send(&message, &peer);
                        }
                    }
                    suppress = true;
                }
                Phase::Done => {
                    self.outbound = None;
                    self.outbound_msg_id = None;
                    self.brain.set_falling(false);
                    self.start_next_outbound_if_idle(now);
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
                    // The ack itself was already sent the moment the delivery
                    // arrived (`handle_received`) - the visitor's walk/handoff
                    // is purely cosmetic and no longer gates it.
                    finished_inbound = true;
                }
                Phase::Handing if !self.inbound_handed_off => {
                    self.inbound_handed_off = true;
                    if let Some(m) = &self.inbound_msg {
                        // Don't dump the message on screen - stash it as unread so
                        // the pet carries the envelope until it's clicked open. The
                        // bubble is just a content-free "you've got mail" beat, and
                        // it outlasts the visitor's short handoff on purpose.
                        self.unread.push_back(m.clone());
                        self.bubble_text = Some(format!("a letter from {} \u{2709}", m.sender_name));
                        self.bubble_until = now + 3.0;
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
        // Carrying the letter on every courier leg it's actually walking, and
        // also while any delivered-but-unread letter is waiting to be opened.
        let couriering = self
            .outbound
            .as_ref()
            .map_or(false, |c| c.phase() != Phase::Away && c.phase() != Phase::Done);
        Some(FrameSprite {
            x: self.pet_x.round() as i32,
            y: self.pet_y.round() as i32,
            anim,
            frame: self.frame_index.min(len.saturating_sub(1)),
            facing_right: self.resident_facing_right(),
            carry_mail: couriering || !self.unread.is_empty(),
            on_horse: couriering && self.outbound_express,
            horse_frame: current_horse_frame(),
            skin: self.state.skin,
            accessories: self.state.accessories.iter().copied().collect(),
        })
    }

    pub fn visitor_sprite(&self) -> Option<FrameSprite> {
        self.inbound.as_ref()?;
        let len = CLIPS.get(&self.visitor_anim).map(|c| c.frames.len()).unwrap_or(1);
        Some(FrameSprite {
            x: self.visitor_x.round() as i32,
            // Pinned to the work-area floor captured when the courier started, not
            // to `pet_y` - the resident pet may be mid-fall (e.g. it just undocked
            // to receive this letter) and shouldn't drag the visitor up with it.
            y: self.visitor_y.round() as i32,
            anim: self.visitor_anim,
            frame: self.visitor_frame_index.min(len.saturating_sub(1)),
            facing_right: self.visitor_facing_right,
            carry_mail: true, // a visitor always shows up holding the letter
            on_horse: self.inbound_express,
            horse_frame: current_horse_frame(),
            // The visiting peer's own chosen look, carried on the delivering
            // `PetMessage` - falls back to classic/no-accessories for a peer on
            // a build that predates skins.
            skin: self.inbound_msg.as_ref().and_then(|m| m.sender_skin).unwrap_or_default(),
            accessories: self.inbound_msg.as_ref().and_then(|m| m.sender_accessories.clone()).unwrap_or_default(),
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
        fn stop(&mut self) {}
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
            self.outbound_pending.is_empty() && !self.outbound_acked.is_empty()
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

        rt.send_message_at("ship it", &["PeerX".to_string()], false, 1000.0);

        rt.tick_at(1000.0 + FAR); // courier walks off-screen -> Away, sends the datagram
        assert_eq!(rt.t_outbound_phase(), Some(Phase::Away));

        let sent = fake.sent.lock().unwrap().clone();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0.kind, Kind::Deliver);
        assert_eq!(sent[0].1, "PeerX");
        let delivered = sent[0].0.clone();

        // The peer acks almost immediately, saying its own visitor animation
        // still needs 2s to finish.
        fake.inbox
            .lock()
            .unwrap()
            .push_back((delivered.make_ack("PeerX", 2.0), "PeerX".into()));
        rt.tick_at(1000.0 + FAR + 0.5);

        assert!(rt.t_ack(), "ack should have been recorded");
        // Too soon after entering Away to honor it yet - the courier waits out
        // the peer's own reported `time_to_return` instead of turning around
        // the instant the ack lands.
        assert_eq!(
            rt.t_outbound_phase(),
            Some(Phase::Away),
            "an instant ack shouldn't skip the recipient's reported wait"
        );

        rt.tick_at(1000.0 + FAR + 2.5); // past the reported wait
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

        rt.send_message_at("hello?", &["PeerX".to_string()], false, 1000.0);
        rt.tick_at(1000.0 + FAR); // -> Away, deadline = (1000+FAR) + 15
        assert_eq!(rt.t_outbound_phase(), Some(Phase::Away));

        rt.tick_at(1000.0 + FAR + 5.0); // still waiting
        assert_eq!(rt.t_outbound_phase(), Some(Phase::Away));

        rt.tick_at(1000.0 + FAR + 16.0); // past the 15s wait, no ack
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

        let msg = PetMessage::deliver("great progress".into(), "Sender".into(), crate::net::Edge::Right, false, SkinId::default(), Vec::new());
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

    #[test]
    fn express_inbound_visitor_rides_the_horse_and_carries_mail() {
        let fake = FakeTransport::with_peers(&["Sender"]);
        let mut rt = new_rt(&fake);

        let msg = PetMessage::deliver(
            "urgent".into(),
            "Sender".into(),
            crate::net::Edge::Right,
            true, // express
            SkinId::default(),
            Vec::new(),
        );
        fake.inbox.lock().unwrap().push_back((msg, "Sender".into()));

        rt.tick_at(2000.0);
        let v = rt.visitor_sprite().expect("a visitor should be walking in");
        assert!(v.on_horse, "an express delivery's visitor rides the horse");
        assert!(v.carry_mail, "the visitor always holds the letter");

        // The ack must round-trip the express flag too.
        rt.tick_at(2000.0 + FAR);
        rt.tick_at(2000.0 + FAR + 3.0);
        rt.tick_at(2000.0 + 2.0 * FAR + 3.0);
        let ack = fake
            .sent
            .lock()
            .unwrap()
            .iter()
            .find(|(m, _)| m.kind == Kind::Ack)
            .cloned()
            .expect("an ack should have been sent back");
        assert!(ack.0.express, "the ack preserves the express flag");
    }

    #[test]
    fn inbound_delivery_is_held_as_unread_not_shown_on_screen() {
        let fake = FakeTransport::with_peers(&["Sender"]);
        let mut rt = new_rt(&fake);

        let body = "the quarterly numbers are in, ping me";
        let msg = PetMessage::deliver(body.into(), "Sender".into(), crate::net::Edge::Right, false, SkinId::default(), Vec::new());
        fake.inbox.lock().unwrap().push_back((msg, "Sender".into()));

        rt.tick_at(3000.0); // spawn the visitor
        rt.tick_at(3000.0 + FAR); // walk it to the handoff -> Phase::Handing

        // The handoff beat must not put the message body on screen.
        if let Some((bubble, _, _)) = rt.bubble() {
            assert!(
                !bubble.contains(body),
                "delivered body must not be shown automatically, got {bubble:?}"
            );
        }

        // It's stashed as unread, and the resident pet now carries the envelope
        // even though it isn't couriering (so `carry_mail` here is the new path).
        assert!(rt.has_unread(), "the letter should be waiting as unread");
        let held = rt.pet_sprite().expect("resident pet visible");
        assert!(held.carry_mail, "pet holds the envelope while a letter is unread");
        assert!(!held.on_horse);

        // Reading it clears the indicator.
        let read = rt.pop_unread().expect("a letter to read");
        assert_eq!(read.text, body);
        assert!(!rt.has_unread());
        assert!(
            !rt.pet_sprite().unwrap().carry_mail,
            "envelope gone once the letter is read"
        );
    }
}
