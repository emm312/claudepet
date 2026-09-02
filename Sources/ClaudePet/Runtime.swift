import AppKit

/// Owns the pet's clock: state, brain, window, and every timer. Everything else
/// in the app is close to pure and driven from here.
final class Runtime {
    private(set) var state: PetState
    private let brain = Brain()
    private nonisolated let distractionDetector = DistractionDetector()
    private var lastAngryBubbleDate: Date = .distantPast

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
    /// Everyone this delivery was addressed to, and who has acked so far -
    /// only meaningful while `outboundCourier` is active.
    private var outboundPendingPeers: Set<String> = []
    private var outboundAckedPeers: Set<String> = []

    /// Active while a visitor's sprite is walking through a handoff.
    private var inboundCourier: Courier?
    private var visitor: VisitorPet?
    private var inboundMessage: PetMessage?
    private var inboundWindowShown = false

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
        state.pet()
        showBubble()
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

    func presentMessageComposer() {
        guard let (text, peers) = MessageComposer.present(peerNames: transport.peerNames) else { return }
        sendMessage(text, to: peers)
    }

    /// Shows an arrived letter in the same letter-styled window used for
    /// composing one, blocking (modally) until the reader taps "OK" or sends
    /// a reply. Runs synchronously mid-tick, same as the compose flow.
    private func presentIncomingMessage(_ message: PetMessage) {
        guard let reply = LetterWindow(message: message).runModal() else { return }
        sendMessage(reply.text, to: reply.peers)
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
    func sendMessage(_ text: String, to peers: [String]) {
        guard outboundCourier == nil, !peers.isEmpty else { return }

        let screen = ScreenGeometry.screen(containing: window.frame.origin)?.visibleFrame
            ?? CGRect(x: 0, y: 0, width: 1440, height: 900)
        let homeX = window.frame.origin.x
        let edge: PetMessage.Edge = (homeX - screen.minX) < (screen.maxX - homeX - window.frame.width) ? .left : .right
        let offScreenX = edge == .right ? screen.maxX + window.frame.width : screen.minX - window.frame.width

        let message = PetMessage.deliver(text: text, senderName: MultipeerLink.localDisplayName, exitEdge: edge)
        outboundMessageID = message.id
        outboundPendingPeers = Set(peers)
        outboundAckedPeers = []
        outboundCourier = Courier.outbound(startX: homeX, homeX: homeX, offScreenX: offScreenX, edge: edge)
        brain.setFalling(false)
        showBubble(force: Dialogue.departLine())
        for peer in peers {
            transport.send(message, to: peer)
        }
    }

    private func handleReceived(_ message: PetMessage, from peerName: String) {
        switch message.kind {
        case .ack:
            guard message.id == outboundMessageID else { return }
            outboundAckedPeers.insert(peerName)
            outboundCourier?.receivedAck()
        case .deliver:
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

        let visitor = VisitorPet(zoom: zoom, y: window.frame.origin.y)
        visitor.setX(offScreenX)
        self.visitor = visitor
        inboundWindowShown = false
        inboundCourier = Courier.inbound(offScreenX: offScreenX, handoffX: handoffX, edge: entryEdge)
    }

    /// Advances any active couriers/visitor by one tick. Returns whether the
    /// resident pet's own movement (gravity/brain) should be suppressed this
    /// tick because it's busy delivering.
    private func tickMessaging(now: Date, dt: TimeInterval) -> Bool {
        var suppressLocalMovement = false

        if let courier = outboundCourier {
            let wasAway = courier.phase == .away
            courier.tick(now: now)
            switch courier.phase {
            case .departing, .returning:
                if wasAway {
                    window.orderFrontRegardless() // just started walking back in
                    if outboundAckedPeers.isEmpty { showBubble(force: Dialogue.deliveryFailedLine()) }
                }
                var origin = window.frame.origin
                origin.x = courier.x
                window.setFrameOrigin(origin)
                suppressLocalMovement = true
            case .away:
                if !wasAway { window.orderOut(nil) } // just finished walking off screen
                suppressLocalMovement = true
            case .done:
                outboundCourier = nil
                outboundMessageID = nil
                brain.setFalling(false)
            default:
                break
            }
        }

        if let courier = inboundCourier, let visitor {
            courier.tick(now: now)
            visitor.setX(courier.x)
            visitor.render(anim: courier.anim, facingRight: courier.facingRight, dt: dt)
            if courier.phase == .done {
                if let message = inboundMessage {
                    transport.send(message.makeAck(), to: message.senderName)
                }
                visitor.dismiss()
                self.visitor = nil
                inboundCourier = nil
                inboundMessage = nil
                startNextDeliveryIfIdle()
            } else if courier.phase == .handing, !inboundWindowShown, let message = inboundMessage {
                inboundWindowShown = true
                presentIncomingMessage(message)
            }
        }

        return suppressLocalMovement
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
            || outboundCourier != nil || inboundCourier != nil
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
            // Gravity first, so the Brain sees this tick's up-to-date isFalling
            // flag (rather than lagging a frame behind) when it picks an anim.
            applyGravity(dt: dt)
            let dx = brain.tick(now: now, mood: state.mood)
            if dx != 0 {
                move(dx: dx)
            }
        }

        advanceFrame(dt: dt)
        view.updateHitTest()
        speechBubble?.follow(above: window.frame)

        if brain.isDistracted, now.timeIntervalSince(lastAngryBubbleDate) > 3.5 {
            lastAngryBubbleDate = now
            showBubble(force: Dialogue.angryLine())
        }

        if now.timeIntervalSince(lastSaveDate) > 20 {
            persistSoon()
        }
    }

    // MARK: - Distraction (Reels rage)

    private func scheduleDistractionCheck() {
        Task.detached(priority: .background) { [weak self] in
            guard let self else { return }
            let distracted = self.distractionDetector.currentlyDistracted()
            await MainActor.run {
                self.brain.setDistracted(distracted)
            }
            // Poll a bit faster while distracted so the pet reacts quickly once
            // the user leaves Reels; slower otherwise since this costs a real
            // (if small) Apple Event round-trip in supported browsers.
            let delay: UInt64 = distracted ? 1_000_000_000 : 2_500_000_000
            try? await Task.sleep(nanoseconds: delay)
            await MainActor.run {
                self.scheduleDistractionCheck()
            }
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
            Task { @MainActor in self?.persistSoon() }
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
