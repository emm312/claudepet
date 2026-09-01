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
messaging interop. On this branch the macOS app also carries
`Sources/ClaudePet/Net/LanUdpLink.swift` and `Net/CompositeTransport.swift`, and one line of
`Runtime.swift` is changed to use them, so a macOS pet and a Windows pet can exchange letters over a
wire-compatible UDP+Bonjour link. Nothing else under `Sources/`, `Tests/`, `Package.swift`, or
`Scripts/` is touched, and none of it is ever pushed to `main`. **These three Swift changes have not
been compiled** (no Swift toolchain on the Windows dev box) — run `swift build` / `swift test` on a
Mac before relying on macOS↔Windows messaging, and note that Mac users must run the app built from
*this* branch, not `main`, for interop.

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
Package.swift, Sources/, Tests/, Scripts/, Resources/sprites/   ← upstream macOS app, DO NOT TOUCH
src-win/                      ← the Windows port (Rust)
  Cargo.toml
  src/
    main.rs                   Win32 message loop, layered overlay window, tray, popup menu, wiring
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
`{ id, kind, text, senderName, exitEdge, sentAt }` — `id` a lowercase dashed UUID, `kind`
`"deliver"`/`"ack"`, `exitEdge` `"left"`/`"right"`, `sentAt` Unix seconds. Mac↔Mac still rides
MultipeerConnectivity; `CompositeTransport` de-dups deliveries by `id` so a peer reachable on both
links isn't served twice.

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

The shippable artifact is the bare `claudepet.exe` (~470 KB release, no runtime DLLs); wrap it in an
NSIS/`cargo-wix` installer only if you want Start-menu integration.

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
