# Network layer audit + fixes (macOS + Windows)

Audit performed 2026-09-02 in response to: multi-recipient sends silently dropping
messages, acks rarely arriving (so the sender reports "message bounced" for
mail that actually got through), and unreliable peer discovery. File-drag
attachment sending was scoped in the same pass but not implemented - see
"Not yet done" below.

## Root causes found (verified in source, both platforms)

### A. Acks were lost because macOS sent every datagram from a throwaway port
`LanUdpLink.send` opened a brand-new `NWConnection` per datagram (a fresh
ephemeral source port) and cancelled it right after sending. Both sides
"upsert the sender's source address" on every inbound datagram, so:
- A Mac -> Windows delivery arrived from ephemeral port P1; Windows stored
  that as the Mac's address. By the time Windows tried to ack, P1 was
  already closed, so the ack was lost.
- The Mac's own reply to a delivery from Windows left from yet another
  ephemeral port, so Windows then learned a second, equally short-lived,
  address for the Mac.

This was the single biggest cause of the reported "acks aren't working".

### B. Acks carried the *original sender's* name, not the acker's
`make_ack()` / `makeAck()` cloned `senderName` from the delivery instead of
stamping the acker's own name. A receiver of an ack therefore upserted
*its own name* into its peer map, and the sender's per-recipient ack
tracking (`outboundAckedPeers` on macOS) recorded the local machine's own
name for every ack instead of the actual peer.

### C. Acks were only sent after the full visitor walk-in/handoff/walk-out
Both platforms acked at the very end of the inbound courier's animation
(several seconds), racing the sender's away-timeout on a wide monitor or a
congested LAN.

### D. No outbound queue - a second send while a courier was out was dropped
Windows: `if self.outbound.is_some() { return; }` with no queue at all, and
`compose.rs` only supported a single recipient (a `CBS_DROPDOWNLIST` combo).
macOS: `guard outboundCourier == nil` similarly dropped a second send, and
while the compose window did support checkbox multi-select, the trip ended
on the *first* ack (`outboundPendingPeers` was written but never read), so
partial delivery failure was invisible.

### E. Discovery / peer-map fragility
- macOS: a routine Bonjour browse refresh wholesale-replaced the endpoint
  map, discarding addresses learned from real traffic; the `NWListener` had
  no failure/restart handling (unlike the browser, which already had one).
- Windows: a fresh mDNS `ServiceResolved` could clobber a known-good learned
  address with a possibly-unreachable advertised one (e.g. a VPN/Hyper-V
  virtual adapter's address on a multi-homed host); the recv thread had no
  shutdown path, so a `stop()` -> `start()` cycle doubled it.

## Fixes applied

1. **One stable source port for every outbound datagram (macOS).**
   `LanUdpLink` now pins every `NWConnection` it opens to originate from the
   listener's own bound port (`NWParameters.requiredLocalEndpoint` +
   `allowLocalEndpointReuse`), so a peer's learned address stays valid across
   a deliver/ack round trip. This is the fix for finding A.

2. **Acks carry the acker's own name.** `PetMessage.make_ack(local_name)` /
   `makeAck(from: localName)` now take the local display name explicitly
   instead of copying it from the delivery. Fixes finding B and, as a side
   effect, makes the source-address upsert on the ack-reception side correct
   too (it was keying off the wrong name before).

3. **Immediate acks.** Both `handle_received` (Rust) and `handleReceived`
   (Swift) now send the ack the instant a `.deliver` message is received,
   decoupled from the (now purely cosmetic) inbound visitor animation. Fixes
   finding C.

4. **Outbound queue + per-recipient ack tracking, both platforms.**
   `Runtime` on both sides now queues a send made while a courier is in
   flight (`outbound_queue` / `outboundQueue`) and starts it the moment the
   previous trip completes. One courier trip still carries a message to every
   recipient at once; `outbound_pending`/`outbound_acked` (Rust) and
   `outboundPendingPeers`/`outboundAckedPeers` (Swift) now drive a
   three-way outcome bubble: full success, "couldn't reach X, Y" for a
   partial ack, or the existing failure line when nobody acked. Windows'
   `compose.rs` was rewritten from a single-selection combo box to one
   checkbox per peer (checked by default), matching the Mac's `LetterWindow`.
   The `Courier` state machine on both platforms also now records an ack that
   arrives mid-`Departing` (`ack_pending` / `ackPending`) so a very fast LAN
   round trip can shorten the trip instead of waiting out the full walk to
   the edge of the screen.

5. **Discovery/address-map hardening.**
   - macOS: the browse-results handler now merges into the existing endpoint
     map (only inserting for peers with no known endpoint yet, and only
     dropping ones the browse no longer reports) instead of replacing it
     wholesale; the `NWListener` gained the same failed/cancelled restart
     behavior the browser already had.
   - Windows: peer addresses are now tracked as `PeerAddrs { advertised,
     learned }`, preferring a `learned` (proven-reachable) address for up to
     5 minutes over `advertised`; a resolve only ever writes `advertised`,
     never overwriting a working `learned` entry. The recv thread now honors
     a shutdown flag with a socket read timeout instead of blocking forever,
     and `peer_names()` filters out the local machine's own name (a
     regression that was possible once acks stopped mis-keying entries under
     the sender's own name).

## Files touched

- `Sources/ClaudePet/Net/LanUdpLink.swift`, `Net/PetMessage.swift`,
  `Pet/Courier.swift`, `Runtime.swift`
- `src-win/src/net/mod.rs`, `net/mdns_udp.rs`, `pet/courier.rs`,
  `runtime.rs`, `compose.rs`, `main.rs`

## Verification

- `swift build` and `swift test` both compile clean on macOS against every
  change above (the `CourierTests` suite, `Runtime`, `LanUdpLink`, and
  `PetMessage` all build and link with the new signatures).
- The Windows/Rust side could not be compiled or tested in this pass - no
  Windows target toolchain was available in the environment it was written
  in. It was written by careful mirroring against the already-verified Swift
  logic and needs `cargo test` / `cargo build --release` on a real Windows
  box (or the `x86_64-pc-windows-gnu` cross target) before shipping.

## Not yet done

**Drag-and-drop file sending** (dragging a file onto the pet on either
platform to send it) was scoped alongside this audit but not implemented.
It needs, on top of everything above:

- A small TCP "pull" file server per platform (sender listens, receiver
  connects and downloads once the courier has an id to fetch), referenced
  from an optional `attachment: { name, size, port }` field added to the
  `PetMessage` wire format.
- Drag-and-drop wiring: `DragAcceptFiles`/`WM_DROPFILES` on the Win32
  layered window (guarded by the existing per-tick cursor-over-pet hit
  test), and `NSDraggingDestination`/`registerForDraggedTypes` on
  `OverlayView` on macOS (reusing its existing alpha hit-test).
  A letter-reader affordance ("Show in Finder" / "Show in Explorer") for
  a received file.

This is a substantial second piece of work in its own right and should be
scoped and reviewed on its own rather than folded into this pass.
