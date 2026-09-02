import AppKit

/// A small, click-through, purely decorative overlay window that shows one
/// courier-prop's frames (the horse's gallop cycle, or the mail's single
/// static image). Mirrors `VisitorPet`'s window setup, generalized to animate
/// when given more than one frame - `main::draw_actor` on the Windows port
/// composites these into one canvas, but this app draws each pet/visitor as
/// its own window, so each prop gets its own small tag-along window instead.
final class CourierProp {
    private let window: OverlayWindow
    private let view: OverlayView
    private let frames: [CGImage]
    private let frameDuration: TimeInterval
    private var frameIndex = 0
    private var frameElapsed: TimeInterval = 0

    /// Every frame is expected pre-rendered at its final on-screen pixel size
    /// (via `PixelArtRenderer.render(grid:zoom:)`), same convention as the
    /// pet's own sprites - no extra display scaling here.
    init(frames: [CGImage], frameDuration: TimeInterval = .infinity) {
        precondition(!frames.isEmpty)
        self.frames = frames
        self.frameDuration = frameDuration
        let first = frames[0]
        let size = CGSize(width: first.width, height: first.height)
        window = OverlayWindow(size: size)
        window.ignoresMouseEvents = true // decoration only, never a click/drag target
        view = OverlayView(frame: CGRect(origin: .zero, size: size))
        window.contentView = view
        window.orderFrontRegardless()
    }

    /// `origin` is the window's bottom-left corner in screen coordinates.
    /// `dt` advances the gallop cycle when there's more than one frame; pass
    /// `0` for a single-frame (static) prop like the mail.
    func setOrigin(_ origin: CGPoint, flippedHorizontally: Bool, dt: TimeInterval = 0) {
        window.setFrameOrigin(origin)
        if frames.count > 1 {
            frameElapsed += dt
            if frameElapsed >= frameDuration {
                frameElapsed = 0
                frameIndex = (frameIndex + 1) % frames.count
            }
        }
        view.setImage(frames[frameIndex], flippedHorizontally: flippedHorizontally)
    }

    func dismiss() {
        window.orderOut(nil)
    }
}
