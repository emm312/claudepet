import AppKit

/// A small, click-through, purely decorative overlay window that shows one
/// static courier-prop image (the horse or the mail) at `scale`x. Mirrors
/// `VisitorPet`'s window setup but for a static image with no animation -
/// `main::draw_actor` on the Windows port composites these into one canvas,
/// but this app draws each pet/visitor as its own window, so each prop gets
/// its own small tag-along window instead.
final class CourierProp {
    private let window: OverlayWindow
    private let view: OverlayView
    private let image: CGImage

    init(image: CGImage, scale: Int) {
        self.image = image
        let size = CGSize(width: image.width * scale, height: image.height * scale)
        window = OverlayWindow(size: size)
        window.ignoresMouseEvents = true // decoration only, never a click/drag target
        view = OverlayView(frame: CGRect(origin: .zero, size: size))
        window.contentView = view
        window.orderFrontRegardless()
    }

    /// `origin` is the window's bottom-left corner in screen coordinates.
    func setOrigin(_ origin: CGPoint, flippedHorizontally: Bool) {
        window.setFrameOrigin(origin)
        view.setImage(image, flippedHorizontally: flippedHorizontally)
    }

    func dismiss() {
        window.orderOut(nil)
    }
}
