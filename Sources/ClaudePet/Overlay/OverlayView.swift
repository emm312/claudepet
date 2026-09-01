import AppKit

/// Hosts the pet's CALayer, draws pixel art with nearest-neighbour scaling, and
/// performs alpha hit-testing so clicks on transparent pixels pass straight
/// through to whatever app is underneath instead of being eaten by our window.
final class OverlayView: NSView {

    private let spriteLayer = CALayer()
    private var currentImage: CGImage?

    /// Called when the user left-clicks a non-transparent pixel of the sprite.
    var onClick: (() -> Void)?
    /// Called on mouse-down on a non-transparent pixel; passes the click point in
    /// window-local coordinates (i.e. the grab offset within the pet's sprite).
    var onDragStart: ((CGPoint) -> Void)?
    /// Called on every drag movement; the owner is expected to reposition the
    /// window using `NSEvent.mouseLocation` and the stored grab offset.
    var onDrag: (() -> Void)?
    var onDragEnd: (() -> Void)?
    /// Called for a right-click on a non-transparent pixel.
    var onRightClick: ((NSEvent) -> Void)?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer = CALayer()
        layer?.masksToBounds = false

        spriteLayer.magnificationFilter = .nearest
        spriteLayer.minificationFilter = .nearest
        spriteLayer.contentsGravity = .resize
        spriteLayer.frame = bounds
        layer?.addSublayer(spriteLayer)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) not supported") }

    override func layout() {
        super.layout()
        spriteLayer.frame = bounds
    }

    /// Poll-based hit-test update. `NSWindow.ignoresMouseEvents` swallows mouse-moved
    /// events entirely while `true`, so we can't rely on tracking areas to know when
    /// the cursor re-enters an opaque pixel - instead this is called once per tick
    /// from the render loop (which already runs at animation cadence) using the
    /// always-available `NSEvent.mouseLocation` global cursor position.
    func updateHitTest() {
        guard let window else { return }
        let screenPoint = NSEvent.mouseLocation
        let windowPoint = window.convertPoint(fromScreen: screenPoint)
        let viewPoint = convert(windowPoint, from: nil)
        let opaque = bounds.contains(viewPoint) && isOpaquePixel(at: viewPoint)
        if window.ignoresMouseEvents != !opaque {
            window.ignoresMouseEvents = !opaque
        }
    }

    /// Swap the currently displayed frame. `image` should be an opaque-where-drawn,
    /// transparent-elsewhere CGImage sized to `bounds` in points * backing scale.
    func setImage(_ image: CGImage, flippedHorizontally: Bool) {
        currentImage = image
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        spriteLayer.contents = image
        spriteLayer.transform = flippedHorizontally
            ? CATransform3DMakeScale(-1, 1, 1)
            : CATransform3DIdentity
        CATransaction.commit()
    }

    // MARK: - Alpha hit-testing

    /// Whether the pixel at this view-local point is opaque (i.e. part of the
    /// visible sprite, as opposed to transparent padding in the bounding box).
    private func isOpaquePixel(at point: CGPoint) -> Bool {
        guard let image = currentImage else { return false }
        let w = image.width, h = image.height
        guard w > 0, h > 0, bounds.width > 0, bounds.height > 0 else { return false }

        let nx = point.x / bounds.width
        // Flip Y: view coords are bottom-left origin, image data top-left.
        let ny = 1 - (point.y / bounds.height)
        guard nx >= 0, nx < 1, ny >= 0, ny < 1 else { return false }

        let px = Int(nx * CGFloat(w))
        let py = Int(ny * CGFloat(h))

        guard let data = image.dataProvider?.data,
              let ptr = CFDataGetBytePtr(data) else { return false }
        let bytesPerPixel = image.bitsPerPixel / 8
        guard bytesPerPixel > 0 else { return false }
        let offset = py * image.bytesPerRow + px * bytesPerPixel
        guard offset >= 0, offset + bytesPerPixel <= CFDataGetLength(data) else { return false }

        // Assume alpha is the last byte of the pixel (standard premultiplied-last layout).
        let alphaOffset = offset + bytesPerPixel - 1
        return ptr[alphaOffset] > 10
    }

    override func mouseDown(with event: NSEvent) {
        onDragStart?(event.locationInWindow)
        onClick?()
    }

    override func mouseDragged(with event: NSEvent) {
        onDrag?()
    }

    override func mouseUp(with event: NSEvent) {
        onDragEnd?()
    }

    override func rightMouseDown(with event: NSEvent) {
        onRightClick?(event)
    }
}
