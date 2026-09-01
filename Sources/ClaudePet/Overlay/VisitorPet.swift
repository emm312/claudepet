import AppKit

/// The other pet's sprite, shown briefly while it delivers a message: a second
/// click-through overlay window rendered with the exact same sprite pipeline as
/// the resident pet, driven by a `Courier` in `.inbound` mode.
///
/// Purely decorative - it never becomes clickable/draggable and is never a
/// mouse target, unlike the resident pet's `OverlayView`.
final class VisitorPet {
    let window: OverlayWindow
    private let view: OverlayView
    private let zoom: Int
    private var frameIndex = 0
    private var frameElapsed: TimeInterval = 0

    init(zoom: Int, y: CGFloat) {
        self.zoom = zoom
        let size = CGSize(
            width: PetSprites.gridSize.width * CGFloat(zoom),
            height: PetSprites.gridSize.height * CGFloat(zoom)
        )
        window = OverlayWindow(size: size)
        window.ignoresMouseEvents = true // decoration only, never a click/drag target
        view = OverlayView(frame: CGRect(origin: .zero, size: size))
        window.contentView = view
        window.setFrameOrigin(CGPoint(x: 0, y: y))
        window.orderFrontRegardless()
    }

    func setX(_ x: CGFloat) {
        var origin = window.frame.origin
        origin.x = x
        window.setFrameOrigin(origin)
    }

    func render(anim: PetMood.AnimState, facingRight: Bool, dt: TimeInterval) {
        guard let clip = PetSprites.clips[anim] else { return }
        frameElapsed += dt
        if frameElapsed >= clip.frameDuration {
            frameElapsed = 0
            frameIndex = (frameIndex + 1) % clip.frames.count
        }
        let idx = min(frameIndex, clip.frames.count - 1)
        let image = PixelArtRenderer.render(grid: clip.frames[idx], zoom: zoom)
        view.setImage(image, flippedHorizontally: !facingRight)
    }

    func dismiss() {
        window.orderOut(nil)
    }
}
