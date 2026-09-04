import AppKit

/// "Claude's Adventure" - a short, self-closing cutscene shown right after a
/// letter is sent, when the compose window's "Watch the journey" box was
/// ticked. The pixel-art backdrop (`Resources/adventure/castle_bridge.bgra`, a
/// raw 192x192 BGRA blob - no JPEG decode; see CLAUDE.md) with the pet walking
/// the stone bridge up to the castle, then a brief hold before the window
/// closes itself. If the letter went express, the pet gallops in on the horse.
///
/// Mirrors `src-win/src/adventure.rs`, with one deliberate divergence: this is
/// **non-modal**. The Rust port runs a nested message pump; here a nested
/// `NSApp.runModal` would starve a `.default`-mode `Timer`, so the window just
/// orders front, animates on a `.common`-mode timer, and `close()`s itself.
final class AdventureWindow: NSWindow {
    /// Scene geometry: everything is authored in a 192x192 space (matching the
    /// backdrop) and the view scales it to its bounds.
    static let sceneSize: CGFloat = 192
    private static let windowScale: CGFloat = 2 // 384x384 content

    /// How long the pet takes to walk the whole bridge, and how long the
    /// finished scene holds on the castle before closing. Kept in sync with
    /// `adventure.rs`'s `WALK_SECONDS` / `HOLD_SECONDS` / express halving.
    static let walkSeconds: TimeInterval = 7
    static let holdSeconds: TimeInterval = 1.6

    private let sceneView: AdventureSceneView

    /// Called once when the window closes itself (or the user closes it) - the
    /// owner uses this to drop its retaining reference.
    var onClose: (() -> Void)?

    init(skin: SkinId, accessories: [AccessoryId], express: Bool) {
        let side = Self.sceneSize * Self.windowScale
        sceneView = AdventureSceneView(
            frame: CGRect(x: 0, y: 0, width: side, height: side),
            skin: skin,
            accessories: accessories,
            express: express
        )
        super.init(
            contentRect: CGRect(x: 0, y: 0, width: side, height: side),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
        title = "Claude's Adventure"
        isReleasedWhenClosed = false
        // Sit one above the pet's own overlay (`OverlayWindow` uses
        // `maximumWindow - 1`) so the cutscene isn't drawn over by the pet.
        level = NSWindow.Level(rawValue: Int(CGWindowLevelForKey(.maximumWindow)))
        contentView = sceneView
        sceneView.onFinished = { [weak self] in self?.close() }
    }

    /// Order the window front and start the animation. Returns immediately -
    /// the window tears itself down when the pet arrives.
    func run() {
        NSApp.activate(ignoringOtherApps: true)
        center()
        makeKeyAndOrderFront(nil)
        sceneView.start()
    }

    override func close() {
        sceneView.stop()
        super.close()
        onClose?()
        onClose = nil
    }
}

/// Draws one frame of the cutscene: backdrop, then the horse (if express), then
/// the pet frame + any worn accessories, positioned along the bridge polyline.
private final class AdventureSceneView: NSView {
    /// The stone bridge as normalised (x, y) knots, y pointing down - the same
    /// polyline as `adventure.rs`'s `PATH`, eyeballed against the backdrop.
    private static let path: [(x: CGFloat, y: CGFloat)] = [
        (0.26, 0.99), // onto the cobbles at the bottom edge
        (0.32, 0.90),
        (0.42, 0.85),
        (0.52, 0.80), // the cobbled path merges into the raised rampart
        (0.62, 0.77),
        (0.71, 0.71), // out to the rampart's rightmost bulge
        (0.70, 0.63), // turning back up toward the keep, now facing left
        (0.58, 0.585),
        (0.47, 0.56), // in where the rampart meets the castle
    ]

    /// Pet sprite zoom in final pixels (the pet grid is 16px; ~64px on the
    /// 384px window matches the on-screen size the Rust scene lands at).
    private static let petZoom = 4
    private static let riderLift: CGFloat = 22

    /// Fake perspective: the pet + horse shrink from full size at the near end
    /// of the path to `farScale` by the castle. Mirrors `adventure.rs`'s
    /// `NEAR_SCALE` / `FAR_SCALE` / `scale_at`.
    private static let nearScale: CGFloat = 1.0
    private static let farScale: CGFloat = 0.45

    private static func scale(at p: CGFloat) -> CGFloat {
        nearScale + (farScale - nearScale) * min(max(p, 0), 1)
    }

    private let skin: SkinId
    private let accessories: [AccessoryId]
    private let express: Bool
    private let backdrop: CGImage?

    private var startDate = Date()
    private var timer: Timer?
    private var arrivedAt: TimeInterval?
    var onFinished: (() -> Void)?

    init(frame: CGRect, skin: SkinId, accessories: [AccessoryId], express: Bool) {
        self.skin = skin
        self.accessories = accessories
        self.express = express
        self.backdrop = Self.loadBackdrop()
        super.init(frame: frame)
        wantsLayer = true
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override var isFlipped: Bool { true } // scene y points down, like adventure.rs

    func start() {
        startDate = Date()
        let t = Timer(timeInterval: 1.0 / 30.0, repeats: true) { [weak self] _ in
            self?.tick()
        }
        // .default mode wouldn't fire while a menu tracks or the window
        // resizes; .common covers those and the normal loop.
        RunLoop.main.add(t, forMode: .common)
        timer = t
    }

    func stop() {
        timer?.invalidate()
        timer = nil
    }

    /// Seconds to cross the whole bridge (express arrives in half the time -
    /// `adventure.rs`'s `EXPRESS_WALK_SECONDS`).
    private var walkSeconds: TimeInterval {
        express ? AdventureWindow.walkSeconds / 2 : AdventureWindow.walkSeconds
    }

    private func tick() {
        let now = Date().timeIntervalSince(startDate)
        if now >= walkSeconds, arrivedAt == nil { arrivedAt = now }
        if let arrivedAt, now - arrivedAt >= AdventureWindow.holdSeconds {
            stop()
            onFinished?()
            return
        }
        needsDisplay = true
    }

    // MARK: - Drawing

    override func draw(_ dirtyRect: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        ctx.interpolationQuality = .none

        if let backdrop {
            // The view is flipped (y-down); flip the CTM back locally so the
            // CGImage draws upright, same trick as `drawImage`.
            ctx.saveGState()
            ctx.translateBy(x: 0, y: bounds.maxY)
            ctx.scaleBy(x: 1, y: -1)
            ctx.draw(backdrop, in: CGRect(origin: .zero, size: bounds.size))
            ctx.restoreGState()
        } else {
            NSColor(calibratedRed: 0.55, green: 0.71, blue: 0.83, alpha: 1).setFill()
            bounds.fill()
        }

        let t = Date().timeIntervalSince(startDate)
        let p = min(1, t / walkSeconds)
        let (nx, ny, facingRight) = Self.pointAlongPath(CGFloat(p))

        let footX = nx * bounds.width
        let footY = ny * bounds.height
        let scale = Self.scale(at: CGFloat(p))

        let petPx = CGFloat(16 * Self.petZoom) * scale
        var petOriginY = footY - petPx

        if express {
            let frames = HorseSprite.frames
            let idx = Int(t / HorseSprite.frameDuration) % max(1, frames.count)
            let hw = CGFloat(22 * Self.petZoom) * scale
            let hh = CGFloat(12 * Self.petZoom) * scale
            let rect = CGRect(x: footX - hw / 2, y: footY - hh, width: hw, height: hh)
            drawImage(frames[idx], in: rect, flippedHorizontally: !facingRight)
            petOriginY -= Self.riderLift * scale
        }

        if let pet = petImage(at: t) {
            let rect = CGRect(x: footX - petPx / 2, y: petOriginY, width: petPx, height: petPx)
            drawImage(pet, in: rect, flippedHorizontally: !facingRight)
        }
    }

    /// Composite the current pet frame (skin + accessories) at `petZoom`.
    private func petImage(at t: TimeInterval) -> CGImage? {
        let anim: PetMood.AnimState = arrivedAt == nil ? .walk : .idle
        guard let def = Skins.all[skin] ?? Skins.all[.classic],
              let clip = def.clips[anim] ?? def.clips[.walk],
              !clip.frames.isEmpty
        else { return nil }
        let fi = Int(t / clip.frameDuration) % clip.frames.count
        let accs = accessories.compactMap { Accessories.all[$0] }
        return PixelArtRenderer.renderComposite(
            grid: clip.frames[fi],
            palette: def.palette,
            accessories: accs,
            zoom: Self.petZoom
        )
    }

    private func drawImage(_ image: CGImage, in rect: CGRect, flippedHorizontally: Bool) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        ctx.saveGState()
        if flippedHorizontally {
            ctx.translateBy(x: rect.midX, y: 0)
            ctx.scaleBy(x: -1, y: 1)
            ctx.translateBy(x: -rect.midX, y: 0)
        }
        // The view is flipped (y-down); flip the CTM back locally so the
        // CGImage draws upright.
        ctx.translateBy(x: 0, y: rect.maxY)
        ctx.scaleBy(x: 1, y: -1)
        ctx.interpolationQuality = .none
        ctx.draw(image, in: CGRect(x: rect.minX, y: 0, width: rect.width, height: rect.height))
        ctx.restoreGState()
    }

    // MARK: - Path

    /// Point + facing along `path` at fraction `p` (0...1) of total arc length.
    static func pointAlongPath(_ p: CGFloat) -> (x: CGFloat, y: CGFloat, facingRight: Bool) {
        let knots = path
        var seg = [CGFloat](repeating: 0, count: knots.count - 1)
        var total: CGFloat = 0
        for i in 0..<knots.count - 1 {
            let dx = knots[i + 1].x - knots[i].x
            let dy = knots[i + 1].y - knots[i].y
            seg[i] = (dx * dx + dy * dy).squareRoot()
            total += seg[i]
        }
        let target = min(max(p, 0), 1) * total
        var acc: CGFloat = 0
        for i in 0..<knots.count - 1 {
            if acc + seg[i] >= target || i == knots.count - 2 {
                let f = seg[i] > 0 ? (target - acc) / seg[i] : 0
                let a = knots[i], b = knots[i + 1]
                return (a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f, b.x >= a.x)
            }
            acc += seg[i]
        }
        let last = knots[knots.count - 1]
        return (last.x, last.y, true)
    }

    // MARK: - Backdrop

    /// Load the raw 192x192 BGRA backdrop - from the app bundle when running as
    /// `ClaudePet.app` (`Scripts/bundle.sh` copies it in), else from the source
    /// tree so `swift run` works too.
    private static func loadBackdrop() -> CGImage? {
        let side = 192
        var url = Bundle.main.url(forResource: "castle_bridge", withExtension: "bgra")
        #if DEBUG
        // `swift run` has no bundle Resources dir - fall back to the source tree.
        // DEBUG-only so a release build with a missing/renamed asset fails
        // visibly here (sky-blue fill) instead of baking this machine's path in.
        if url == nil {
            url = URL(fileURLWithPath: #filePath)
                .deletingLastPathComponent() // UI
                .deletingLastPathComponent() // ClaudePet
                .deletingLastPathComponent() // Sources
                .deletingLastPathComponent() // repo root
                .appendingPathComponent("Resources/adventure/castle_bridge.bgra")
        }
        #endif
        guard let url, let data = try? Data(contentsOf: url), data.count == side * side * 4 else { return nil }
        guard let provider = CGDataProvider(data: data as CFData) else { return nil }
        // Bytes are B,G,R,255. As a little-endian 32-bit word that's 0xFFRRGGBB,
        // i.e. skip-first alpha in byte-order-32-little.
        let bitmapInfo = CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipFirst.rawValue)
            .union(.byteOrder32Little)
        return CGImage(
            width: side,
            height: side,
            bitsPerComponent: 8,
            bitsPerPixel: 32,
            bytesPerRow: side * 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: bitmapInfo,
            provider: provider,
            decode: nil,
            shouldInterpolate: false,
            intent: .defaultIntent
        )
    }
}
