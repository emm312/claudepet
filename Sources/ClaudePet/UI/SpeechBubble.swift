import AppKit

/// A tiny always-on-top, click-through speech bubble that hovers above the pet
/// and auto-dismisses. Same window level/collection behavior as the pet itself
/// so it never gets left behind on a Space switch.
final class SpeechBubble {
    private let window: OverlayWindow
    private let label: NSTextField
    private var dismissWorkItem: DispatchWorkItem?
    private(set) var isVisible = false

    init() {
        let size = CGSize(width: 160, height: 40)
        window = OverlayWindow(size: size)

        let background = NSVisualEffectView(frame: CGRect(origin: .zero, size: size))
        background.material = .popover
        background.state = .active
        background.wantsLayer = true
        background.layer?.cornerRadius = 10
        background.layer?.masksToBounds = true

        label = NSTextField(labelWithString: "")
        label.font = .systemFont(ofSize: 12, weight: .medium)
        label.alignment = .center
        label.frame = CGRect(x: 4, y: 4, width: size.width - 8, height: size.height - 8)
        label.lineBreakMode = .byWordWrapping
        label.maximumNumberOfLines = 2

        background.addSubview(label)
        window.contentView = background
        window.ignoresMouseEvents = true
        window.alphaValue = 0
    }

    func show(text: String, above petFrame: CGRect, duration: TimeInterval = 3.0) {
        label.stringValue = text
        reposition(above: petFrame)
        window.orderFront(nil)
        isVisible = true

        dismissWorkItem?.cancel()
        NSAnimationContext.runAnimationGroup { ctx in
            ctx.duration = 0.15
            window.animator().alphaValue = 1
        }

        let work = DispatchWorkItem { [weak self] in self?.dismiss() }
        dismissWorkItem = work
        DispatchQueue.main.asyncAfter(deadline: .now() + duration, execute: work)
    }

    /// Called every runtime tick while visible so the bubble rides along above
    /// the pet as it walks, dances, or gets dragged, instead of staying pinned
    /// to wherever it happened to be when `show` fired.
    func follow(above petFrame: CGRect) {
        guard isVisible else { return }
        reposition(above: petFrame)
    }

    private func reposition(above petFrame: CGRect) {
        let size = window.frame.size
        let origin = CGPoint(
            x: petFrame.midX - size.width / 2,
            y: petFrame.maxY + 6
        )
        window.setFrameOrigin(origin)
    }

    private func dismiss() {
        isVisible = false
        NSAnimationContext.runAnimationGroup({ ctx in
            ctx.duration = 0.2
            window.animator().alphaValue = 0
        }, completionHandler: { [weak self] in
            Task { @MainActor in self?.window.orderOut(nil) }
        })
    }
}
