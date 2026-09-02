# CLAUDE.md

## ⚠️ Branch rule — read first

**All commits and pushes on this checkout go to the `windows` branch only. Never push to `main`.**

`main` is the upstream **macOS** app (Swift / AppKit) and must not be modified from here. This
branch (`windows`) adds a **Windows port in Rust** alongside the untouched Swift sources.

A structural guard is already configured in this clone:

```
git config remote.origin.push refs/heads/windows:refs/heads/windows
```

so a bare `git push` can only ever update `origin/windows`. If you clone fresh, re-run that line and
`git checkout windows` before doing anything else. Do not `git push origin main` and do not open PRs
against `main`.

The Swift sources are upstream and stay that way **with one exception**: the cross-platform
messaging interop, which has grown since this branch was cut. On this branch the macOS app also
carries `Sources/ClaudePet/Net/LanUdpLink.swift` and `Net/CompositeTransport.swift`; `Runtime.swift`
uses them (`CompositeTransport([MultipeerLink(), LanUdpLink()])`); `Net/PetMessage.swift` gains an
optional-decoded `express` field so the horse/express flag survives the round trip; and
`Overlay/CourierProp.swift` + `Pet/CourierProps.swift` add the horse/mail rendering (see below).
Nothing else under `Tests/`, `Package.swift`, or `Scripts/` is touched, and none of it is ever pushed
to `main`.

This carve-out was cut from `main` before three commits landed there (`dc69ae9` incoming-message
letter window, `ba30cfd` multi-recipient sends, `171fe15` the flaky-discovery fix); all three have
since been cherry-picked onto `windows` so the interop carve-out doesn't regress behind upstream.
`PeerTransport.stop()`, `CompositeTransport.stop()`, and `LanUdpLink.stop()` all exist for the same
reason `MultipeerLink.stop()` does — a clean goodbye on quit so a quick relaunch doesn't see a stale
peer.

**Mac-side horse/mail rendering**: `Pet/CourierProps.swift` bakes `horse.jpg`/`mail.jpg` (base64,
embedded directly in Swift source — no bundle resources or SwiftPM resource pipeline) into
chroma-keyed, cropped, nearest-resized RGBA `CGImage`s at first use, replicating what
`src-win/build.rs` does at compile time. `Overlay/CourierProp.swift` is a small click-through
overlay window (mirrors `VisitorPet`'s setup) that shows one baked image; `Runtime.swift` positions
one horse + one mail prop alongside the resident pet while it's out on an express delivery, and
another pair alongside the visitor while it's delivering one in — the visitor rides the horse
whenever `PetMessage.express` is true, matching `src-win/src/runtime.rs`'s `on_horse` logic. Unlike
the Windows port (one composited canvas via `main::draw_actor`), each prop is its own small window,
since this app draws every pet/visitor as a separate `OverlayWindow` — positions approximate, not
pixel-identical to, the Rust placement math. `LetterWindow`/`MessageComposer` now carry the express
checkbox (`compose.rs`'s Mac-side counterpart) so both directions can send/receive express.

**These Swift changes have not been compiled** as of this note (no Swift toolchain on the Windows
dev box, and the latest round wasn't verified with `swift build`/`swift test` before committing) —
run both on a Mac before relying on macOS↔Windows messaging or the horse/mail rendering, and note
that Mac users must run the app built from *this* branch, not `main`, for interop.

---

## What this project is

ClaudePet is a desktop pet: a small pixel-art critter that lives on top of your other windows, walks
around, perches on the title bars of open windows, falls with gravity when its perch disappears, gets
grumpy when you doomscroll, and can walk a short text message over to another ClaudePet on the LAN.

- `main` branch: the original **macOS** app — Swift 6.2, AppKit, MultipeerConnectivity, macOS 13+.
- `windows` branch (here): a **from-scratch Windows port in Rust**. Nothing links the two builds at
  runtime; the port re-implements the behaviour, not the binary.

Only the pure game logic could be carried over conceptually (mood/stat decay, the behaviour brain,
the courier walk-off/walk-on state machine, the dialogue pools, the pixel-grid sprites). Everything
that touches the screen, the tray, autostart, or the network is native Windows code.

## Layout

```
CLAUDE.md                     ← this file
.github/workflows/release.yml ← tag `v*` → build + upload release assets (auto-update source)
Package.swift, Sources/       ← upstream macOS app (only the messaging-interop carve-out is touched)
Tests/, Scripts/, Resources/sprites/   ← upstream macOS app, untouched
horse.jpg, mail.jpg           ← pixel-art courier props (baked into the Windows binary at build time)
src-win/                      ← the Windows port (Rust)
  Cargo.toml
  publish.ps1                 build + `gh release` a new version for the in-app updater
  build.rs                    decode/crop/downscale horse.jpg + mail.jpg → embedded RGBA sprites
  src/
    main.rs                   Win32 message loop, layered overlay window, tray, popup menu, wiring
    update.rs                 GitHub-release auto-update: WinHTTP download, BCrypt sha256, self-swap
    runtime.rs                the tick loop + gravity — mirrors Sources/ClaudePet/Runtime.swift
    layered_window.rs         WS_EX_LAYERED overlay over the whole virtual screen; UpdateLayeredWindow
                              with a premultiplied-BGRA DIB; transparent pixels pass clicks through
    render.rs                 software rasteriser: pixel-grid sprites (nearest-neighbour zoom),
                              speech bubble, visitor pet
    tray.rs                   Shell_NotifyIcon tray + menu (stats, actions, peers, autostart, quit)
                              — mirrors UI/StatusItem.swift
    autostart.rs              HKCU\...\Run registry value — mirrors UI/LoginItemManager.swift
    compose.rs                minimal native compose window (EDIT + peer COMBOBOX + Send/Cancel)
                              — mirrors UI/MessageComposer.swift + UI/LetterWindow.swift
    geometry.rs               monitor enumeration + work area (excl. taskbar), clamp-on-screen
                              — mirrors Overlay/ScreenGeometry.swift
    ledges.rs                 EnumWindows + GetWindowRect + DWMWA_CLOAKED filter → walkable ledges
                              — mirrors Overlay/WindowLedges.swift
    distraction.rs            GetForegroundWindow → process image name + window title match
                              — mirrors Pet/DistractionDetector.swift (weaker: no browser tab URL)
    pet/
      pet_state.rs            Mood, PetState{hunger,energy,happiness,cleanliness}, decay, store
                              — mirrors Pet/PetState.swift
      brain.rs               behaviour state machine, tick(now, mood) -> dx — mirrors Pet/Brain.swift
      courier.rs             outbound/inbound walk state machine — mirrors Pet/Courier.swift
      dialogue.rs            buzzword speech-bubble pools — mirrors Pet/Dialogue.swift
      sprites.rs             the 16×16 pixel grids + clip table — mirrors Pet/Sprites.swift
    net/
      mod.rs                 `trait PeerTransport` + `struct PetMessage` — mirrors Net/PeerTransport
                              .swift + Net/PetMessage.swift
      mdns_udp.rs            MdnsUdpTransport: mdns-sd discovery of `_claudepet._udp.local.` +
                              JSON PetMessage datagrams over UDP — replaces Net/MultipeerLink.swift
  installer/                  ← the install wizard (own crate, workspace member)
    src/main.rs               claudepet-setup.exe: embeds the release claudepet.exe (include_bytes!)
                              and runs a 4-page Win32 wizard; also `--silent` / `--uninstall`
```

## Coordinate system

The Rust port uses **Win32 screen coordinates throughout: origin top-left, +Y downward.** The Swift
original is AppKit bottom-left-origin, +Y up. The flip is done once, conceptually, during the port:
gravity accelerates toward **+Y**, a ledge's `y` is the **top edge** of a window in screen space,
and "land on a ledge" means the pet's foot `y` meets `ledge.y` from above. Do not sprinkle sign
flips at call sites — keep every position, velocity, and ledge in this one convention.

## Networking

MultipeerConnectivity has no Windows equivalent, so `net/mdns_udp.rs` is a fresh implementation:
`mdns-sd` advertises/browses `_claudepet._udp.local.` on the LAN, and `PetMessage`s are JSON UDP
datagrams to the peer's advertised address:port. Peer identity is the advertised instance name
(matches how the Swift `PeerTransport` keys peers by display name). Override the local name with the
`CLAUDEPET_PEER_NAME` env var to run two instances on one box.

**macOS ↔ Windows messaging** works via the same link: the macOS app on this branch runs
`CompositeTransport([MultipeerLink(), LanUdpLink()])`, where `LanUdpLink` speaks the identical
Bonjour type (`_claudepet._udp`) and JSON shape. Wire format (both sides): a flat object
`{ id, kind, text, senderName, exitEdge, sentAt, express }` — `id` a lowercase dashed UUID, `kind`
`"deliver"`/`"ack"`, `exitEdge` `"left"`/`"right"`, `sentAt` Unix seconds, `express` a bool
(optional — omitted reads as `false`). The Rust `net::tests::wire_contract_*` test pins the exact
byte string. Mac↔Mac still rides MultipeerConnectivity; `CompositeTransport` de-dups deliveries by
`id` so a peer reachable on both links isn't served twice.

**Courier props** (`src-win/src/props.rs`, baked by `build.rs` from `horse.jpg` / `mail.jpg`): the
resident pet holds the mail on every courier leg it walks; an **express** send (compose checkbox, or
`express: true` on the wire) makes it ride the horse at `EXPRESS_SPEED_MULT` × courier speed. The
receiving screen's visitor pet always holds the mail and rides the horse when the delivery was
express. `FrameSprite.carry_mail` / `.on_horse` drive `main::draw_actor` (horse under → pet → mail
over).

## Build / run / test

From `src-win/`:

```
cargo test                       # pure-logic + runtime unit tests (39)
cargo run                        # debug run of the pet
cargo build --release            # → target/release/claudepet.exe  (single file, no runtime deps)
CLAUDEPET_PEER_NAME=DeskA cargo run   # second instance for local messaging tests
```

### Installer

```
cargo build --release -p claudepet          # build the app first (installer embeds it)
cargo build --release -p claudepet-setup    # → target/release/claudepet-setup.exe (~700 KB)
```

`claudepet-setup.exe` — per-user, no admin. GUI wizard (Welcome → options → progress → done), or
`--silent` for an unattended install (Start-menu + desktop shortcuts, no autostart), or
`--uninstall` (`--uninstall --silent` for scripted removal). Installs to
`%LOCALAPPDATA%\Programs\ClaudePet`, registers in Add/Remove Programs, and leaves a copy of itself as
`uninstall.exe`. `%APPDATA%\ClaudePet\state.json` (the pet's saved state) is intentionally kept on
uninstall. The uninstaller resolves the install dir from the ARP `InstallLocation` value, never from
its own path.

Builds with `stable-x86_64-pc-windows-msvc` (the default on this machine). No Swift, no Xcode, and
no Visual Studio *IDE* — but the MSVC target still needs a linker (`link.exe`), which comes from the
**Visual Studio Build Tools** or the standalone **Windows SDK**; `rustc` locates it automatically via
`vswhere`. If neither is installed, either add one (`winget install Microsoft.VisualStudio.2022.BuildTools`
with the "Desktop development with C++" workload) or switch to the GNU toolchain
(`rustup default stable-x86_64-pc-windows-gnu`), which links with bundled MinGW and needs no SDK.

The shippable artifact is the bare `claudepet.exe` (~560 KB release, no runtime DLLs); wrap it in an
NSIS/`cargo-wix` installer only if you want Start-menu integration.

### Auto-update (`src-win/src/update.rs`)

A background thread checks `https://api.github.com/repos/emm312/claudepet/releases/latest` ~15 s
after launch, then every 6 h. If `tag_name` is newer than `CARGO_PKG_VERSION` **and** the app is
running from a real install (there's a sibling `uninstall.exe`), it downloads the release's
`claudepet.exe` asset over WinHTTP, verifies it against the `claudepet.exe.sha256` asset (BCrypt
SHA-256; refuses on mismatch, proceeds if the asset is absent), and stages it as `claudepet.new.exe`.
The UI thread then either applies it right away (an 8 s "shipping vX…" bubble, then rename the live
exe to `claudepet.old.exe`, move the new one in, relaunch, exit) when **Automatic updates** is on
(tray toggle, default on, persisted in `state.json`), or just surfaces **Install update vX now** in
the menu. On next launch `update::cleanup()` deletes the leftover `.old.exe`. No crates — WinHTTP +
BCrypt only; TLS is the OS's.

### Publishing a release

`src-win/publish.ps1` (needs `gh auth login`): builds app + installer, writes `claudepet.exe.sha256`,
and `gh release create`/`upload`s all three to `v<Cargo.toml version>` on the `windows` branch. The
asset names `claudepet.exe` / `claudepet.exe.sha256` are load-bearing — `update::check()` finds them
by name. Auto-upload alternative: push a `v*` tag and `.github/workflows/release.yml` builds on a
Windows runner and attaches the same assets. Bump `version` in `src-win/Cargo.toml` before tagging;
that's what running clients compare against.

## Porting rules

- Keep `src-win/src/pet/*` and `net/mod.rs` pure and `#[cfg(test)]`-covered. The four Swift test
  files under `Tests/ClaudePetTests/` are the spec — assertions are ported as-is **except** two in
  `CourierTests` (`outboundTimesOutAndReturnsWithoutAck`, `inboundArrivesHandsOffThenLeaves`), whose
  timing numbers contradict `Courier.swift`'s own "reset the deadline when the phase is entered"
  logic; the Rust ports re-anchor those to the phase transition (see the comment in
  `pet/courier.rs`'s test module). `runtime.rs` additionally carries integration tests for the
  ack-vs-timeout messaging interaction using an in-process fake transport and an injected clock
  (`tick_at`).
- Constants come straight from the Swift source (gravity 1400, terminal 1600, courier speed 90,
  handoff 2.2 s, away timeout 10 s, decay 3/2/1.5/1 per hour, 12 h catch-up cap, nap cap 5 min,
  30 fps moving / 8 fps idle, sprite zoom 5).
- If a macOS capability has no clean Windows equivalent, degrade explicitly and note it here rather
  than faking it (currently: distraction detection is title/process based only).
