# CLAUDE.md

## Branch history

This project started as a macOS-only app on `main` (Swift / AppKit). The `windows` branch added a
Windows port in Rust (`src-win/`) alongside the Swift sources, plus a small macOS-side interop
carve-out for cross-platform pet-to-pet messaging. `windows` has since been merged into `main`, so
`main` now carries both platforms; there is no longer a push restriction separating the two branches.

The Swift sources are upstream and stay that way **with one exception**: the cross-platform
messaging interop, which has grown since this branch was cut. On this branch the macOS app also
carries `Sources/ClaudePet/Net/LanUdpLink.swift` and `Net/CompositeTransport.swift`; `Runtime.swift`
uses them (`CompositeTransport([MultipeerLink(), LanUdpLink()])`); `Net/PetMessage.swift` gains an
optional-decoded `express` field so the horse/express flag survives the round trip;
`Overlay/CourierProp.swift` + `Pet/HorseSprite.swift` + `Pet/MailSprite.swift` add the horse/mail
rendering (see below); and `Runtime.swift` + `Overlay/OverlayView.swift` + `Pet/Courier.swift` + `UI/StatusItem.swift` carry
the click-to-open received-letter UX that matches the Windows port (see "Receiving a letter" under
Networking — no auto-opening reader, mail in the pet's hand, `handoffDuration` 0.5s, a "Read Letter…"
item on the pet's right-click menu and the status item). `Tests/ClaudePetTests/CourierTests.swift`
tracks the shorter handoff. Nothing else under `Tests/`, `Package.swift`, or `Scripts/` is touched,
and none of it is ever pushed to `main`.

This carve-out was cut from `main` before three commits landed there (`dc69ae9` incoming-message
letter window, `ba30cfd` multi-recipient sends, `171fe15` the flaky-discovery fix); all three have
since been cherry-picked onto `windows` so the interop carve-out doesn't regress behind upstream.
`PeerTransport.stop()`, `CompositeTransport.stop()`, and `LanUdpLink.stop()` all exist for the same
reason `MultipeerLink.stop()` does — a clean goodbye on quit so a quick relaunch doesn't see a stale
peer.

**Horse/mail rendering — pixel art on both platforms, no JPEGs.** The horse and mail used to be baked
from `horse.jpg`/`mail.jpg` (chroma-keyed and cropped at build time). Both platforms have since
switched to hand-authored pixel grids in the same style as the pet's own sprites, and `horse.jpg`/
`mail.jpg` have been deleted along with every baking step:

- **Swift**: `Pet/HorseSprite.swift` and `Pet/MailSprite.swift` author the horse and mail as pixel
  grids (flat color blocks via the shared `Palette`/`PixelArtRenderer`, `.` = transparent), consistent
  with `Pet/Sprites.swift`. The horse has a 2-frame gallop cycle (`HorseSprite.frames`/
  `frameDuration`, drawn at the pet's own `zoom`); the mail is a single static envelope (drawn at a
  smaller zoom of 2 - at the pet's zoom of 5 it would render almost as big as the pet itself).
  `Overlay/CourierProp.swift` is a small click-through overlay window (mirrors `VisitorPet`'s setup)
  that plays one prop's frames.
- **Rust**: `pet/sprites.rs` carries the identical grids (`HORSE_FRAMES`/`HORSE_FRAME_DURATION`,
  `MAIL_GRID`), rendered with the existing `Canvas::blit_grid` (the same palette-index renderer the
  pet's own sprites use) instead of the old RGBA `Canvas::blit_rgba`. `props.rs`, `build.rs`, and the
  `image` build-dependency are gone entirely - nothing decodes a JPEG anymore. `main.rs::draw_actor`
  picks a gallop frame from the wall clock (`current_horse_frame()` in `runtime.rs`) since the
  poll-based render loop has no dedicated animation-frame state to thread through.

Both platforms lift the pet above its normal ground position while it's on the horse's back
(`HorseSprite.riderLift` in Swift, the existing `pet_y - 16` in `main::draw_actor` on Rust) so it
reads as sitting on top rather than overlapping the horse at the same height. On the Swift side this
is a real (if temporary) move of the resident/visitor's actual window position, restored to the exact
pre-trip ground height the instant the courier finishes (`outboundGroundY` in `Runtime.swift`) so
gravity resumes correctly - not just a rendering offset.

`Runtime.swift` positions one horse + one mail prop alongside the resident pet while it's out on an
express delivery, and another pair alongside the visitor while it's delivering one in — the visitor
rides the horse whenever `PetMessage.express` is true, matching `src-win/src/runtime.rs`'s `on_horse`
logic. Unlike the Windows port (one composited canvas via `main::draw_actor`), each Swift prop is its
own small window, since this app draws every pet/visitor as a separate `OverlayWindow` — positions
approximate, not pixel-identical to, the Rust placement math. `LetterWindow`/`MessageComposer` carry
the express checkbox (`compose.rs`'s Mac-side counterpart) so both directions can send/receive
express; the express speed multiplier is 3x on both platforms (`Courier.expressSpeedMultiplier` /
`src-win/src/runtime.rs`'s `EXPRESS_SPEED_MULT` — keep these in sync if either changes).

**Not yet compiled**: the click-to-open received-letter changes to `Runtime.swift`, `Overlay/
OverlayView.swift`, `Pet/Courier.swift`, `UI/StatusItem.swift` and `Tests/ClaudePetTests/
CourierTests.swift` were written on the Windows dev box, which has no Swift toolchain — **none of them
have been through `swift build` or `swift test`**. Do both on a Mac before relying on macOS↔Windows
messaging. (An earlier note claimed the horse/mail + transport-`stop()` carve-out had been
`swift build`-verified on a Mac; that was a prior session's claim about `546aa0c`-era code and does
not cover anything in this change.) The Windows (Rust) side of this change *is* built and tested —
`cargo test`, 46 passing — and was run on a real desktop.

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
src-win/                      ← the Windows port (Rust)
  Cargo.toml
  publish.ps1                 build + `gh release` a new version for the in-app updater
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

**Courier props** (`src-win/src/pet/sprites.rs`'s `HORSE_FRAMES`/`MAIL_GRID` - pixel grids, not baked
JPEGs; see the horse/mail section above): the resident pet holds the mail on every courier leg it
walks; an **express** send (compose checkbox, or `express: true` on the wire) makes it ride the horse
at `EXPRESS_SPEED_MULT` × courier speed. The receiving screen's visitor pet always holds the mail and
rides the horse when the delivery was express. `FrameSprite.carry_mail` / `.on_horse` /
`.horse_frame` drive `main::draw_actor` (horse under → pet → mail over).

**Receiving a letter — same UX on both platforms.** A delivered letter no longer pops a reader open
(the mac app used to, `dc69ae9`). The inbound visitor does a short "touch and go" handoff
(`HANDOFF_DURATION` / `Courier.handoffDuration` cut 2.2s → 0.5s on both) and leaves straight away; the
resident pet then **carries the envelope** as the "click me" cue and a content-free "a letter from X
✉" bubble shows briefly. Clicking the pet opens the oldest unread letter; "Read Letter…" in the
tray / right-click menu (shown only when one's waiting) is the same path.

- **Windows** (`src-win/`): `Runtime::unread` (a `VecDeque<PetMessage>`); `FrameSprite.carry_mail`
  stays true while it's non-empty. The click target is the envelope specifically —
  `runtime::cursor_over_mail`, a padded rect from the shared `runtime::mail_rect` (also the draw
  origin, so hit test and sprite can't drift), OR'd into the overlay hit test and the `WM_LBUTTON*`
  branches via `App::mail_down` — which posts `WM_APP_READ_LETTER` to open `letter.rs`'s themed reader
  (`App::modal_open` guards re-entrancy and forces overlay click-through while it's up). `tray::
  ID_READ_LETTER` is the docked-pet escape hatch. `letter.rs` mirrors the *read* mode of
  `UI/LetterWindow.swift` — paper fill, clay border + wax-seal dot, serif "A Letter Arrived" title,
  "From:" line, body, plain **OK** + clay-pill **Reply** (inline: toggles the body editable,
  retitles, sends on the same window) — in a normal captioned Win32 frame rather than AppKit's
  borderless rounded panel. `Runtime::unread` is in-memory only: `update.rs`'s swap-and-relaunch
  silently drops an unread letter. Debug builds only: `CLAUDEPET_FAKE_LETTER=1` seeds one at startup
  so `letter.rs` can be eyeballed with `cargo run`.
- **macOS** (`Sources/`, carve-out): `Runtime.unreadLetters: [PetMessage]`; `updateUnreadMailProp`
  keeps the existing `MailSprite` `CourierProp` beside the pet while it's non-empty. The whole pet is
  the click target (`OverlayView` gains `onClickUp` — mouse-up within 4px of mouse-down, i.e. not a
  drag, mirroring the Windows `!app.moved` check; `handlePet` yields the click to the letter while one
  waits). `openUnreadLetter` runs the existing `LetterWindow(message:)` reader modally, so the reader
  itself is unchanged. `unreadLetters` is in-memory only.

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
after launch, then every 20 min. If `tag_name` is newer than `CARGO_PKG_VERSION` **and** the app is
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
and `gh release create`/`upload`s all three to `v<Cargo.toml version>` on `main` (the branch has
carried both platforms since `windows` merged, so this no longer targets a `windows` branch). The
asset names `claudepet.exe` / `claudepet.exe.sha256` are load-bearing — `update::check()` finds them
by name. `.github/workflows/release.yml` is the CI alternative — manually triggered (Actions tab, or
`gh workflow run release.yml -f version=0.2.0`; omitting `version` uses the root `VERSION` file) rather
than firing on every tag push, so a release only happens when someone actually asks for one. Bump
`version` in `src-win/Cargo.toml` **and** the root `VERSION` file together before releasing — one tag
(`v<VERSION>`) carries both platforms' assets, so they must agree.

## macOS updater and shared versioning

A root `VERSION` file (e.g. `0.1.0`) is the single source of truth for the release tag both
platforms publish under. `Scripts/bundle.sh` reads it into `Info.plist`'s
`CFBundleShortVersionString` instead of a hardcoded literal. `src-win/Cargo.toml`'s own `version`
field still drives the Windows build directly (`CARGO_PKG_VERSION`); keep the two in sync by hand
when cutting a release, same as the note above.

The Swift app gained its own GitHub-Releases updater, `Sources/ClaudePet/Update/Updater.swift` — a
port of `src-win/src/update.rs` (URLSession + CryptoKit instead of WinHTTP + BCrypt, still zero
external dependencies) reading the same `emm312/claudepet` releases. `Runtime.scheduleUpdateCheck()`
mirrors `main.rs`'s update thread exactly: 15s initial delay, 20min recheck, dedupe by staged version,
gated to real `/Applications` installs (`Updater.isRealInstall`, the Mac analog of "sibling
`uninstall.exe` present"). On a newer release it downloads `ClaudePet-mac.zip`, verifies it against
`ClaudePet-mac.zip.sha256` when present (CryptoKit SHA-256; asset names are load-bearing, matched by
`Updater.swift`'s lookups the same way the Windows names are), unzips via `ditto`, and clears the
downloaded app's quarantine flag via `xattr` (needed since the app is only ad-hoc/dev signed, not
notarized). `PetState.autoUpdatesEnabled` (default `true`, decoded with a fallback so older
`state.json` files without the field don't reset every stat) gates auto-apply exactly like Windows'
`auto_update`: on, `Runtime.performUpdateCheck()` shows a "shipping vX — relaunching…" bubble and
calls `applyPendingUpdateNow()` 8s later (same delay as Windows' announce-then-apply); off, the
update just sits staged until the "Install update vX now" status-item menu item (hidden unless a
staged update exists, mirroring `readLetterItem`'s visibility pattern) is clicked, which applies
immediately with no bubble — matching Windows' `ID_UPDATE_NOW` path exactly.
`Updater.applyAndRelaunch` trashes the running `.app` bundle, moves the staged one into place, and
relaunches — the macOS equivalent of the Windows exe rename/swap, but without a leftover-`.old.exe`
cleanup step since `FileManager.trashItem` handles that.

`Scripts/publish.sh` (needs `gh auth login`) is the macOS mirror of `src-win/publish.ps1`: runs
`Scripts/bundle.sh`, zips the app with `ditto` into `ClaudePet-mac.zip`, writes
`ClaudePet-mac.zip.sha256`, and publishes both to `v<VERSION>` on `emm312/claudepet` (same tag the
Windows assets go to). `.github/workflows/release.yml` is `workflow_dispatch`-only (manually run from
the Actions tab or `gh workflow run release.yml -f version=X.Y.Z`, defaulting to the root `VERSION`
file) with three jobs: `create-release` (resolves the version, creates the empty release first so the
two build jobs below can't race each other creating it), `build-windows`, and `build-macos`
(`macos-latest`, runs the same bundle+zip+checksum steps as `Scripts/publish.sh`) — both build jobs
upload to the tag `create-release` resolved.

**CI code signing.** `build-macos`'s runner has no Apple Development certificate, so left alone
`Scripts/bundle.sh` falls back to ad-hoc signing there — per that script's own comment, an ad-hoc
cdhash changes every build, so every auto-update would look like a brand-new app to TCC and silently
revoke the Accessibility grant distraction detection needs (surfacing as "gotta remove the old
permission to give the new app permission" after updating). Fixed by giving CI a *stable* identity of
its own: a self-signed code-signing certificate (CN "ClaudePet CI Signing", 10-year expiry, not tied
to any Apple Developer account — signing doesn't need trust-chain validation, only a private key) is
stored as the `MACOS_CI_CERT_P12`/`MACOS_CI_CERT_PASSWORD` repo secrets (base64 p12 + its passphrase).
`build-macos`'s "Import signing certificate" step imports it into a throwaway keychain and exports
`CODESIGN_IDENTITY=ClaudePet CI Signing`, which `bundle.sh` already honors (it prefers the env var
over its own `security find-identity` lookup) — so every CI build gets the same designated
requirement, and TCC grants survive updates the same way they do across the user's own local rebuilds
with their real Apple Development cert. The certificate's private key lives only in those two GitHub
secrets, nowhere in the repo.

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
  away timeout 10 s, decay 3/2/1.5/1 per hour, 12 h catch-up cap, nap cap 5 min, 30 fps moving /
  8 fps idle, sprite zoom 5). **Exception**: `courier::HANDOFF_DURATION` is 0.5 s here, not the
  Swift 2.2 s — the inbound visitor no longer lingers to show the message (the resident pet keeps
  the letter; see the Networking section). `CourierTests`' `inboundArrivesHandsOffThenLeaves` was
  already a re-anchored exception and its Rust port tracks the shorter dwell.
- If a macOS capability has no clean Windows equivalent, degrade explicitly and note it here rather
  than faking it (currently: distraction detection is title/process based only; receiving a letter
  shows mail-in-hand + click-to-open instead of auto-opening the reader).
