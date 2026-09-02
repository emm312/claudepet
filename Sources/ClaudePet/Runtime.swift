import AppKit

/// Owns the pet's clock: state, brain, window, and every timer. Everything else
/// in the app is close to pure and driven from here.
final class Runtime {
    private(set) var state: PetState
    private let brain = Brain()
    private nonisolated let distractionDetector = DistractionDetector()
    private var rampage: Rampage?
    /// Bubble cadence for the rare fallback path where the pet is distracted
    /// but has no `Rampage` (no window geometry) - see `applySighting`.
    private var lastFallbackAngryBubbleDate: Date = .distantPast

    private let zoom = 5
    private let window: OverlayWindow
    private let view: OverlayView

    private var ledges: [Ledge] = []
    private var fallVelocity: CGFloat = 0
    private static let gravityAcceleration: CGFloat = 1400 // pt/s^2
    private static let terminalFallSpeed: CGFloat = 1600   // pt/s
    private var frameIndex = 0
    private var frameElapsed: TimeInterval = 0
    private var lastTickDate = Date()
    private var lastSaveDate = Date()
    private var sleepStartDate: Date?
    private static let napDuration: TimeInterval = 5 * 60

    private var tickTimer: Timer?
    private var ledgeTimer: Timer?
    private var dragOffset: CGPoint = .zero

    private var speechBubble: SpeechBubble?
    private var statusItemController: StatusItemController?

    /// Held for the app's whole lifetime. Without this, macOS App Naps this
    /// LSUIElement process whenever its window is hidden (e.g. mid-delivery,
    /// `orderOut(nil)` above), throttling the tick `Timer` for minutes at a
    /// time - the pet looks stuck away even though the status item still
    /// responds instantly, since that's driven by a direct AppKit click
    /// rather than the throttled timer loop.
    private let napAssertion = ProcessInfo.processInfo.beginActivity(
        options: [.userInitiatedAllowingIdleSystemSleep, .latencyCritical],
        reason: "Pet animation must keep ticking while off-screen"
    )

    // MARK: - Pet-to-pet messaging

    // windows-branch: MultipeerConnectivity for macOS<->macOS, plus a
    // wire-compatible LAN UDP link for macOS<->Windows. See CLAUDE.md.
    private let transport: PeerTransport = CompositeTransport([MultipeerLink(), LanUdpLink()])

    /// Active while the local pet is out delivering a message.
    private var outboundCourier: Courier?
    private var outboundMessageID: UUID?
    /// The message + recipients queued for this delivery. Held rather than
    /// sent immediately - `tickMessaging` fires it once the courier actually
    /// clears the screen (`.away`), so a peer can't ack back before the local
    /// state machine is ready to act on it (see `Courier.receivedAck()`'s
    /// `.away`-only guard).
    private var outboundMessage: PetMessage?
    private var outboundRecipients: [String] = []
    /// Everyone this delivery was addressed to, and who has acked so far -
    /// only meaningful while `outboundCourier` is active.
    private var outboundPendingPeers: Set<String> = []
    private var outboundAckedPeers: Set<String> = []
    /// Mirrors `outboundCourier?.phase == .away` as of the end of the last
    /// `tickMessaging` call. `Courier.receivedAck()` can flip `.away ->
    /// .returning` asynchronously (the moment an ack arrives over the
    /// network, not on the next tick), so a fresh `courier.phase == .away`
    /// snapshot taken at the top of `tickMessaging` can no longer tell
    /// "was away last tick" from "already walking back" - it reads `false`
    /// either way once the ack beat the tick. Keeping the previous tick's
    /// state here instead of re-deriving it fixes that: the pet's window
    /// used to never come back on-screen (staying `orderOut` forever) once
    /// acks started arriving reliably, since the ack-triggered transition
    /// never passed through the `wasAway` check that shows it again.
    private var outboundWasAway = false
    /// Sends made while a previous trip is in flight - one `Courier` trip per
    /// message regardless of recipient count, so a second `sendMessage` while
    /// away is queued rather than silently dropped.
    private var outboundQueue: [(PetMessage, [String])] = []
    /// Express (horse) delivery - only meaningful while `outboundCourier` is
    /// active. windows-branch feature; see HorseSprite.swift/MailSprite.swift.
    private var outboundExpress = false
    /// The resident window's true ground height, captured when a delivery
    /// starts - while riding express, the window is visually lifted above
    /// this by `HorseSprite.riderLift` so the pet sits on the horse's back;
    /// restored the moment the courier finishes so gravity resumes correctly.
    private var outboundGroundY: CGFloat = 0
    private var horseProp: CourierProp?
    private var mailProp: CourierProp?

    /// Active while a visitor's sprite is walking through a handoff.
    private var inboundCourier: Courier?
    private var visitor: VisitorPet?
    private var inboundMessage: PetMessage?
    /// Set once the visitor reaches the handoff point and the letter has been
    /// stashed into `unreadLetters` - guards that append from repeating per tick.
    private var inboundHandedOff = false
    /// Delivered letters not yet read. The resident pet holds the mail sprite
    /// while this is non-empty; clicking the pet opens the oldest one (see
    /// `openUnreadLetter`). In-memory only, like the Windows port's
    /// `Runtime::unread` - a deliberate change from auto-opening the reader.
    private var unreadLetters: [PetMessage] = []
    /// The visitor's true ground height, captured once at spawn.
    private var visitorGroundY: CGFloat = 0
    /// The visitor's actual vertical position (mirrors `VisitorPet`'s own
    /// `y`) - equal to `visitorGroundY`, or lifted by `HorseSprite.riderLift`
    /// for the whole trip when the delivery is express, so it sits on the
    /// horse's back rather than overlapping it at the same height.
    private var visitorBaseY: CGFloat = 0
    private var visitorHorseProp: CourierProp?
    private var visitorMailProp: CourierProp?

    /// Deliveries that arrived while another was already playing out, so they
    /// hand off one at a time instead of stacking visitors.
    private var pendingDeliveries: [PetMessage] = []

    var peerNames: [String] { transport.peerNames }

    init() {
        state = PetStateStore.load()

        let size = CGSize(
            width: PetSprites.gridSize.width * CGFloat(zoom),
            height: PetSprites.gridSize.height * CGFloat(zoom)
        )
        window = OverlayWindow(size: size)
        view = OverlayView(frame: CGRect(origin: .zero, size: size))
        window.contentView = view

        let screen = NSScreen.main?.visibleFrame
            ?? CGRect(x: 0, y: 0, width: 1440, height: 900)
        let startOrigin = CGPoint(x: screen.midX - size.width / 2, y: screen.minY)
        window.setFrameOrigin(startOrigin)
        window.orderFrontRegardless()

        wireViewCallbacks()
        refreshLedges()
        renderCurrentFrame()

        speechBubble = SpeechBubble()
        statusItemController = StatusItemController(runtime: self)

        registerForSystemNotifications()
        startTimers()
        scheduleDistractionCheck()
        wireTransport()
    }

    private func wireTransport() {
        transport.onPeersChanged = { [weak self] _ in
            self?.statusItemController?.peersDidChange()
        }
        transport.onReceive = { [weak self] message, peerName in
            self?.handleReceived(message, from: peerName)
        }
        transport.start()
    }

    // MARK: - View callbacks

    private func wireViewCallbacks() {
        view.onClick = { [weak self] in self?.handlePet() }
        view.onClickUp = { [weak self] in self?.openUnreadLetter() }
        view.onDragStart = { [weak self] point in
            self?.dragOffset = point
            self?.brain.beginDrag()
        }
        view.onDrag = { [weak self] in self?.handleDrag() }
        view.onDragEnd = { [weak self] in
            self?.brain.endDrag()
            self?.snapToNearestLedgeAfterDrag()
        }
        view.onRightClick = { [weak self] event in self?.showContextMenu(for: event) }
    }

    private func handlePet() {
        guard !isDragActive else { return }
        // A waiting letter takes over the click; petting resumes once it's been
        // opened. Opening happens on mouse-*up* (`openUnreadLetter`) so a drag
        // can't trigger it and the press's event sequence finishes before the
        // modal `LetterWindow` runs.
        guard unreadLetters.isEmpty else { return }
        state.pet()
        showBubble()
    }

    /// Is a delivered letter waiting to be read? Drives the "Read Letter…" menu
    /// items' visibility.
    var hasUnreadLetter: Bool { !unreadLetters.isEmpty }

    /// Open the oldest unread letter, if any - wired to the pet's clean-click
    /// (mouse-up, no drag), the pet's right-click menu, and the status item.
    func openUnreadLetter() {
        guard !isDragActive, !unreadLetters.isEmpty else { return }
        presentIncomingMessage(unreadLetters.removeFirst())
    }

    private var isDragActive = false

    private func handleDrag() {
        isDragActive = true
        let mouseScreen = NSEvent.mouseLocation
        let newOrigin = CGPoint(x: mouseScreen.x - dragOffset.x, y: mouseScreen.y - dragOffset.y)
        window.setFrameOrigin(newOrigin)
    }

    private func snapToNearestLedgeAfterDrag() {
        isDragActive = false
        // Let the fall/landing logic on the next ticks settle it onto a ledge
        // naturally rather than teleporting.
    }

    private func showContextMenu(for event: NSEvent) {
        let menu = NSMenu()
        if !unreadLetters.isEmpty {
            menu.addItem(withTitle: "Read Letter…", action: #selector(menuReadLetter), keyEquivalent: "")
                .target = self
            menu.addItem(.separator())
        }
        menu.addItem(withTitle: "Feed", action: #selector(menuFeed), keyEquivalent: "")
            .target = self
        menu.addItem(withTitle: "Play", action: #selector(menuPlay), keyEquivalent: "")
            .target = self
        menu.addItem(withTitle: "Clean", action: #selector(menuClean), keyEquivalent: "")
            .target = self
        menu.addItem(.separator())
        menu.addItem(withTitle: "Send Message…", action: #selector(menuSendMessage), keyEquivalent: "")
            .target = self
        menu.addItem(.separator())
        // Right on the pet itself, not just buried in the menu bar - quitting
        // should never require hunting for a tiny status item.
        menu.addItem(withTitle: "Quit ClaudePet", action: #selector(menuQuit), keyEquivalent: "")
            .target = self
        NSMenu.popUpContextMenu(menu, with: event, for: view)
    }

    @objc private func menuFeed() { feed() }
    @objc private func menuPlay() { play() }
    @objc private func menuClean() { clean() }
    @objc private func menuQuit() { NSApp.terminate(nil) }
    @objc private func menuSendMessage() { presentMessageComposer() }
    @objc private func menuReadLetter() { openUnreadLetter() }

    func presentMessageComposer() {
        guard let (text, peers, express) = MessageComposer.present(peerNames: transport.peerNames) else { return }
        sendMessage(text, to: peers, express: express)
    }

    /// Shows an arrived letter in the same letter-styled window used for
    /// composing one, blocking (modally) until the reader taps "OK" or sends
    /// a reply. Runs synchronously mid-tick, same as the compose flow.
    private func presentIncomingMessage(_ message: PetMessage) {
        guard let reply = LetterWindow(message: message).runModal() else { return }
        sendMessage(reply.text, to: reply.peers, express: reply.express)
    }

    // MARK: - Actions (also used by the status bar menu)

    func feed() {
        state.feed()
        brain.celebrate()
        showBubble(force: Dialogue.celebrationLine())
        persistSoon()
    }

    func play() {
        state.play()
        showBubble(force: "leveraging some blue-sky thinking")
        persistSoon()
    }

    func clean() {
        state.clean()
        showBubble(force: "optimizing my core competencies")
        persistSoon()
    }

    private func showBubble(force text: String? = nil) {
        let text = text ?? Dialogue.line(for: state.mood)
        speechBubble?.show(text: text, above: window.frame)
    }

    // MARK: - Pet-to-pet messaging

    /// Sends `text` to every peer in `peers` in one trip: the local pet walks
    /// off screen, each peer's pet shows it and hands back an ack, and this
    /// pet walks home once at least one ack (or the timeout) comes back.
    /// Ignored if the pet is already out delivering something, or if `peers`
    /// is empty.
    func sendMessage(_ text: String, to peers: [String], express: Bool = false) {
        guard !peers.isEmpty else { return }
        let message = PetMessage.deliver(text: text, senderName: MultipeerLink.localDisplayName, exitEdge: outboundExitEdge(), express: express)
        outboundQueue.append((message, peers))
        startNextOutboundIfIdle()
    }

    private func outboundExitEdge() -> PetMessage.Edge {
        let screen = ScreenGeometry.screen(containing: window.frame.origin)?.visibleFrame
            ?? CGRect(x: 0, y: 0, width: 1440, height: 900)
        let homeX = window.frame.origin.x
        return (homeX - screen.minX) < (screen.maxX - homeX - window.frame.width) ? .left : .right
    }

    /// Starts the next queued send once the courier is free. A send made while
    /// a previous trip is in flight is queued here (`sendMessage` above)
    /// rather than silently dropped, and starts the moment the courier lands.
    private func startNextOutboundIfIdle() {
        guard outboundCourier == nil, !outboundQueue.isEmpty else { return }
        let (message, peers) = outboundQueue.removeFirst()

        let screen = ScreenGeometry.screen(containing: window.frame.origin)?.visibleFrame
            ?? CGRect(x: 0, y: 0, width: 1440, height: 900)
        let homeX = window.frame.origin.x
        let edge = message.exitEdge
        let offScreenX = edge == .right ? screen.maxX + window.frame.width : screen.minX - window.frame.width

        outboundMessageID = message.id
        outboundMessage = message
        outboundRecipients = peers
        outboundPendingPeers = Set(peers)
        outboundAckedPeers = []
        outboundExpress = message.express
        outboundGroundY = window.frame.origin.y
        outboundWasAway = false
        outboundCourier = Courier.outbound(startX: homeX, homeX: homeX, offScreenX: offScreenX, edge: edge, express: message.express)
        brain.setFalling(false)
        showBubble(force: message.express ? "saddling up - taking this one express" : Dialogue.departLine())
    }

    private func handleReceived(_ message: PetMessage, from peerName: String) {
        switch message.kind {
        case .ack:
            guard message.id == outboundMessageID else { return }
            outboundPendingPeers.remove(peerName)
            outboundAckedPeers.insert(peerName)
            outboundCourier?.receivedAck()
        case .deliver:
            // Ack right away - the sender's timeout races real time, not the
            // visitor's walk-in/handoff/walk-out animation, so a slow or wide
            // screen no longer makes a delivered letter look "bounced".
            transport.send(message.makeAck(from: MultipeerLink.localDisplayName), to: peerName)
            pendingDeliveries.append(message)
            startNextDeliveryIfIdle()
        }
    }

    private func startNextDeliveryIfIdle() {
        guard inboundCourier == nil, !pendingDeliveries.isEmpty else { return }
        let message = pendingDeliveries.removeFirst()
        inboundMessage = message

        let screen = ScreenGeometry.screen(containing: window.frame.origin)?.visibleFrame
            ?? CGRect(x: 0, y: 0, width: 1440, height: 900)
        let entryEdge = message.exitEdge.opposite
        let width = window.frame.width
        let offScreenX = entryEdge == .right ? screen.maxX + width : screen.minX - width
        let handoffOffset: CGFloat = 60
        let handoffX = entryEdge == .right
            ? window.frame.origin.x + handoffOffset
            : window.frame.origin.x - handoffOffset

        visitorGroundY = window.frame.origin.y
        visitorBaseY = visitorGroundY + (message.express ? HorseSprite.riderLift : 0)
        let visitor = VisitorPet(zoom: zoom, y: visitorBaseY)
        visitor.setX(offScreenX)
        self.visitor = visitor
        inboundHandedOff = false
        inboundCourier = Courier.inbound(offScreenX: offScreenX, handoffX: handoffX, edge: entryEdge, express: message.express)
    }

    /// Advances any active couriers/visitor by one tick. Returns whether the
    /// resident pet's own movement (gravity/brain) should be suppressed this
    /// tick because it's busy delivering.
    private func tickMessaging(now: Date, dt: TimeInterval) -> Bool {
        var suppressLocalMovement = false

        if let courier = outboundCourier {
            let wasAway = outboundWasAway
            courier.tick(now: now)
            outboundWasAway = courier.phase == .away
            switch courier.phase {
            case .departing, .returning:
                if wasAway {
                    window.orderFrontRegardless() // just started walking back in
                    if outboundAckedPeers.isEmpty {
                        showBubble(force: Dialogue.deliveryFailedLine())
                    } else if !outboundPendingPeers.isEmpty {
                        let missed = outboundPendingPeers.sorted().joined(separator: ", ")
                        showBubble(force: "couldn't reach \(missed)")
                    }
                }
                var origin = window.frame.origin
                origin.x = courier.x
                origin.y = outboundExpress ? outboundGroundY + HorseSprite.riderLift : outboundGroundY
                window.setFrameOrigin(origin)
                suppressLocalMovement = true
            case .away:
                if !wasAway {
                    window.orderOut(nil) // just finished walking off screen
                    // Only send once the pet has actually left the screen -
                    // sending at compose time let a fast LAN ack race back
                    // before the courier reached `.away`, and `receivedAck()`
                    // silently ignores acks outside that phase.
                    if let message = outboundMessage {
                        for peer in outboundRecipients {
                            transport.send(message, to: peer)
                        }
                    }
                }
                suppressLocalMovement = true
            case .done:
                // Land exactly on the ground the trip started from, undoing
                // any express-ride lift before gravity/dragging resumes.
                window.setFrameOrigin(CGPoint(x: courier.x, y: outboundGroundY))
                outboundCourier = nil
                outboundMessageID = nil
                outboundMessage = nil
                outboundRecipients = []
                brain.setFalling(false)
                startNextOutboundIfIdle()
            default:
                break
            }
        }

        if let courier = outboundCourier, courier.phase != .away {
            updateResidentProps(origin: window.frame.origin, facingRight: courier.facingRight, dt: dt)
        } else if !unreadLetters.isEmpty {
            // Not couriering, but a delivered letter is waiting - keep the mail
            // in the pet's hand as the "click me" cue.
            updateUnreadMailProp(origin: window.frame.origin, facingRight: currentFacingRight)
        } else {
            hideResidentProps()
        }

        if let courier = inboundCourier, let visitor {
            courier.tick(now: now)
            visitor.setX(courier.x)
            visitor.render(anim: courier.anim, facingRight: courier.facingRight, dt: dt)
            updateVisitorProps(x: courier.x, express: courier.express, facingRight: courier.facingRight, dt: dt)
            if courier.phase == .done {
                // The ack itself was already sent the moment the delivery
                // arrived (`handleReceived`) - the visitor's walk/handoff is
                // purely cosmetic and no longer gates it.
                visitor.dismiss()
                self.visitor = nil
                inboundCourier = nil
                inboundMessage = nil
                hideVisitorProps()
                startNextDeliveryIfIdle()
            } else if courier.phase == .handing, !inboundHandedOff, let message = inboundMessage {
                // Don't pop the reader open. Stash the letter and let the pet
                // carry the envelope until it's clicked (mirrors the Windows
                // port); the bubble is a content-free "you've got mail" beat.
                inboundHandedOff = true
                unreadLetters.append(message)
                showBubble(force: "a letter from \(message.senderName) \u{2709}")
            }
        } else {
            hideVisitorProps()
        }

        return suppressLocalMovement
    }

    // MARK: - Courier props (horse + mail)

    /// `origin` is the resident pet window's bottom-left corner. Mirrors
    /// `main::draw_actor` on the Windows port (horse under, pet, mail over),
    /// with each prop as its own tag-along window instead of one composited
    /// canvas.
    private func updateResidentProps(origin: CGPoint, facingRight: Bool, dt: TimeInterval) {
        let spriteSize = PetSprites.gridSize.width * CGFloat(zoom)
        if outboundExpress {
            let prop = horseProp ?? CourierProp(frames: HorseSprite.frames, frameDuration: HorseSprite.frameDuration)
            horseProp = prop
            let w = CGFloat(HorseSprite.frames[0].width)
            // Ground level, not `origin.y` - the pet's window is already
            // lifted by `HorseSprite.riderLift` while riding (see
            // `tickMessaging`), and the horse itself should stay planted.
            prop.setOrigin(CGPoint(x: origin.x + spriteSize / 2 - w / 2, y: outboundGroundY), flippedHorizontally: !facingRight, dt: dt)
        } else if let prop = horseProp {
            prop.dismiss()
            horseProp = nil
        }

        let mailProp = mailProp ?? CourierProp(frames: [MailSprite.image])
        self.mailProp = mailProp
        let mailW = CGFloat(MailSprite.image.width)
        let mailX = facingRight ? origin.x + spriteSize - mailW - 4 : origin.x + 4
        mailProp.setOrigin(CGPoint(x: mailX, y: origin.y + 22), flippedHorizontally: !facingRight)
    }

    private func hideResidentProps() {
        horseProp?.dismiss(); horseProp = nil
        mailProp?.dismiss(); mailProp = nil
    }

    /// The resident pet keeps the mail in hand while a delivered letter is
    /// waiting to be read - no horse, this isn't a courier leg. Positioned like
    /// the carried mail in `updateResidentProps`.
    private func updateUnreadMailProp(origin: CGPoint, facingRight: Bool) {
        if let prop = horseProp { prop.dismiss(); horseProp = nil }
        let spriteSize = PetSprites.gridSize.width * CGFloat(zoom)
        let prop = mailProp ?? CourierProp(frames: [MailSprite.image])
        mailProp = prop
        let mailW = CGFloat(MailSprite.image.width)
        let mailX = facingRight ? origin.x + spriteSize - mailW - 4 : origin.x + 4
        prop.setOrigin(CGPoint(x: mailX, y: origin.y + 22), flippedHorizontally: !facingRight)
    }

    /// A visitor always carries the mail; it rides the horse only when the
    /// delivery it's carrying was sent express (`courier.express`, threaded
    /// from `PetMessage.express` in `startNextDeliveryIfIdle`).
    private func updateVisitorProps(x: CGFloat, express: Bool, facingRight: Bool, dt: TimeInterval) {
        let spriteSize = PetSprites.gridSize.width * CGFloat(zoom)
        if express {
            let prop = visitorHorseProp ?? CourierProp(frames: HorseSprite.frames, frameDuration: HorseSprite.frameDuration)
            visitorHorseProp = prop
            let w = CGFloat(HorseSprite.frames[0].width)
            // Ground level, not `visitorBaseY` - the visitor's own window is
            // already lifted by `HorseSprite.riderLift` while riding (see
            // `startNextDeliveryIfIdle`), and the horse should stay planted.
            prop.setOrigin(CGPoint(x: x + spriteSize / 2 - w / 2, y: visitorGroundY), flippedHorizontally: !facingRight, dt: dt)
        } else if let prop = visitorHorseProp {
            prop.dismiss()
            visitorHorseProp = nil
        }

        let visitorMailProp = visitorMailProp ?? CourierProp(frames: [MailSprite.image])
        self.visitorMailProp = visitorMailProp
        let mailW = CGFloat(MailSprite.image.width)
        let mailX = facingRight ? x + spriteSize - mailW - 4 : x + 4
        visitorMailProp.setOrigin(CGPoint(x: mailX, y: visitorBaseY + 22), flippedHorizontally: !facingRight)
    }

    private func hideVisitorProps() {
        visitorHorseProp?.dismiss(); visitorHorseProp = nil
        visitorMailProp?.dismiss(); visitorMailProp = nil
    }

    // MARK: - Timers

    private func startTimers() {
        scheduleTick()
        ledgeTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.refreshLedges() }
        }
    }

    /// Ticks at a cadence appropriate to what the pet is currently doing, so it
    /// burns near-zero CPU while idle/asleep and stays smooth while walking.
    private func scheduleTick() {
        tickTimer?.invalidate()
        let isFastMotion = brain.anim == .walk || brain.anim == .angry || brain.anim == .fall || brain.anim == .dance
            || outboundCourier != nil || inboundCourier != nil || rampage != nil
        let interval: TimeInterval = isFastMotion ? (1.0 / 30.0) : (1.0 / 8.0)
        tickTimer = Timer.scheduledTimer(withTimeInterval: interval, repeats: false) { [weak self] _ in
            Task { @MainActor in
                self?.tick()
                self?.scheduleTick()
            }
        }
        if let tickTimer {
            RunLoop.main.add(tickTimer, forMode: .common)
        }
    }

    private func tick() {
        let now = Date()
        let dt = now.timeIntervalSince(lastTickDate)
        lastTickDate = now

        state.tick(now: now)
        // A nap is capped at a fixed real-time length rather than left to
        // energy recovery alone - otherwise a pet that fell asleep near zero
        // energy would take hours (or, before this, forever) to wake back up.
        if brain.anim == .sleep {
            if sleepStartDate == nil { sleepStartDate = now }
            if now.timeIntervalSince(sleepStartDate!) >= Self.napDuration {
                state.energy = max(state.energy, 65) // enough to clear the tired threshold
                brain.wake(now: now)
                sleepStartDate = nil
            }
        } else {
            sleepStartDate = nil
        }
        if state.energy > 60 { brain.wake(now: now) }

        let deliveryBusy = tickMessaging(now: now, dt: dt)

        if !isDragActive && !deliveryBusy {
            if let rampage {
                // Rampage owns position outright while active - no gravity,
                // no ledges, it flies wherever the browser window is.
                _ = brain.tick(now: now, mood: state.mood) // keeps anim == .angry current
                let next = rampage.tick(now: now, dt: dt)
                window.setFrameOrigin(ScreenGeometry.clampOrigin(next, size: window.frame.size))
                if rampage.shouldSpeakNow(now: now) {
                    showBubble(force: Dialogue.angryLine(tier: rampage.tier))
                }
            } else {
                // Gravity first, so the Brain sees this tick's up-to-date
                // isFalling flag (rather than lagging a frame behind) when it
                // picks an anim.
                applyGravity(dt: dt)
                let dx = brain.tick(now: now, mood: state.mood)
                if dx != 0 {
                    move(dx: dx)
                }
                if brain.isDistracted, now.timeIntervalSince(lastFallbackAngryBubbleDate) > 3.5 {
                    lastFallbackAngryBubbleDate = now
                    showBubble(force: Dialogue.angryLine(tier: .furious))
                }
            }
        }

        advanceFrame(dt: dt)
        view.updateHitTest()
        speechBubble?.follow(above: window.frame)

        if now.timeIntervalSince(lastSaveDate) > 20 {
            persistSoon()
        }
    }

    // MARK: - Distraction (Reels rage)

    private func scheduleDistractionCheck() {
        Task.detached(priority: .background) { [weak self] in
            guard let self else { return }
            let sighting = self.distractionDetector.currentSighting()
            await MainActor.run {
                self.applySighting(sighting)
            }
            // Poll a bit faster while distracted so the pet reacts quickly once
            // the user leaves Reels; slower otherwise since this costs a real
            // (if small) Accessibility round-trip in supported browsers.
            let delay: UInt64 = sighting != nil ? 1_000_000_000 : 2_500_000_000
            try? await Task.sleep(nanoseconds: delay)
            await MainActor.run {
                self.scheduleDistractionCheck()
            }
        }
    }

    private func applySighting(_ sighting: DistractionSighting?) {
        brain.setDistracted(sighting != nil)

        guard let sighting else {
            if rampage != nil {
                rampage = nil
                brain.setFalling(false) // let gravity settle it onto a ledge again
            }
            return
        }

        // No usable window geometry this poll (AX read failed transiently) -
        // keep whatever rampage/frame we already have rather than tearing it
        // down over one bad sample.
        guard let frame = sighting.frame else { return }

        if let rampage {
            rampage.updateTarget(frame: frame)
        } else {
            rampage = Rampage(frame: frame, petSize: window.frame.size, currentPosition: window.frame.origin)
        }
    }

    private func move(dx: CGFloat) {
        var origin = window.frame.origin
        origin.x += dx
        origin = ScreenGeometry.clampOrigin(origin, size: window.frame.size)
        window.setFrameOrigin(origin)
    }

    /// If the ledge under the pet has vanished (e.g. a window closed or moved),
    /// let it tumble with real acceleration until it lands on the next ledge
    /// below, rather than sliding down at a fixed rate.
    private func applyGravity(dt: TimeInterval) {
        let frame = window.frame
        let footX = frame.midX
        let footY = frame.minY

        if let ledge = WindowLedges.ledgeBelow(x: footX, y: footY + 1, in: ledges),
           abs(ledge.y - footY) < 1 {
            if fallVelocity != 0 {
                fallVelocity = 0
                brain.setFalling(false)
            }
            return // already standing on something
        }

        // No ledge at all below (shouldn't normally happen - the screen floor
        // is always a fallback ledge) - nothing to fall onto, so hold in place.
        guard let target = WindowLedges.ledgeBelow(x: footX, y: footY, in: ledges) else { return }

        brain.setFalling(true)
        fallVelocity = min(fallVelocity + CGFloat(dt) * Self.gravityAcceleration, Self.terminalFallSpeed)

        var origin = frame.origin
        let proposedY = origin.y - fallVelocity * CGFloat(dt)
        if proposedY <= target.y {
            origin.y = target.y
            fallVelocity = 0
            brain.setFalling(false)
        } else {
            origin.y = proposedY
        }
        window.setFrameOrigin(origin)
    }

    private func refreshLedges() {
        ledges = WindowLedges.currentLedges()
    }

    // MARK: - Rendering

    /// The courier's own anim/facing take over while it's actively moving the
    /// resident pet's window; otherwise the Brain drives rendering as usual.
    private var currentAnim: PetMood.AnimState {
        guard let outboundCourier, outboundCourier.phase != .away else { return brain.anim }
        return outboundCourier.anim
    }

    private var currentFacingRight: Bool {
        if let rampage { return rampage.facingRight }
        guard let outboundCourier, outboundCourier.phase != .away else { return brain.facingRight }
        return outboundCourier.facingRight
    }

    private func advanceFrame(dt: TimeInterval) {
        guard let clip = PetSprites.clips[currentAnim] else { return }
        frameElapsed += dt
        if frameElapsed >= clip.frameDuration {
            frameElapsed = 0
            frameIndex = (frameIndex + 1) % clip.frames.count
        }
        renderCurrentFrame()
    }

    private func renderCurrentFrame() {
        guard let clip = PetSprites.clips[currentAnim] else { return }
        let idx = min(frameIndex, clip.frames.count - 1)
        let image = PixelArtRenderer.render(grid: clip.frames[idx], zoom: zoom)
        view.setImage(image, flippedHorizontally: !currentFacingRight)
    }

    // MARK: - Persistence & lifecycle

    private func persistSoon() {
        lastSaveDate = Date()
        PetStateStore.save(state)
    }

    private func registerForSystemNotifications() {
        // NotificationCenter callbacks are handed to us as non-isolated closures even
        // though we requested the `.main` queue, so each body hops back onto the main
        // actor explicitly before touching any of our main-actor-isolated state.
        let wc = NSWorkspace.shared.notificationCenter
        wc.addObserver(forName: NSWorkspace.willSleepNotification, object: nil, queue: .main) { [weak self] _ in
            Task { @MainActor in self?.persistSoon() }
        }
        wc.addObserver(forName: NSWorkspace.didWakeNotification, object: nil, queue: .main) { [weak self] _ in
            Task { @MainActor in self?.lastTickDate = Date() }
        }

        NotificationCenter.default.addObserver(
            forName: NSApplication.willTerminateNotification, object: nil, queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.persistSoon()
                self?.transport.stop()
            }
        }

        NotificationCenter.default.addObserver(
            forName: NSApplication.didChangeScreenParametersNotification, object: nil, queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                guard let self else { return }
                let clamped = ScreenGeometry.clampOrigin(self.window.frame.origin, size: self.window.frame.size)
                self.window.setFrameOrigin(clamped)
                self.refreshLedges()
            }
        }
    }
}
