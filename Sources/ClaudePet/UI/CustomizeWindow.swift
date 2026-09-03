import AppKit

/// A small window for picking the pet's skin and worn accessories: a live
/// preview of the idle animation, a ◀ ▶ pair to step through `SkinId.allCases`
/// (wrapping), and one checkbox per `AccessoryId`. Every change applies
/// immediately through `Runtime.setSkin`/`setAccessory` - closing the window
/// is not a separate "commit" step, matching how `autoUpdatesEnabled` applies
/// the instant its menu item is toggled.
///
/// A standard titled `NSWindow` rather than `LetterWindow`'s bespoke
/// borderless paper styling - kept deliberately simple given this is a
/// settings surface, not part of the letter-delivery fiction.
final class CustomizeWindow: NSWindow {
    private weak var runtime: Runtime?
    private let preview = NSImageView(frame: CGRect(x: 0, y: 0, width: 160, height: 160))
    private let skinLabel = NSTextField(labelWithString: "")
    private var accessoryCheckboxes: [AccessoryId: NSButton] = [:]

    private var previewSkin: SkinId
    private var frameIndex = 0
    private var previewTimer: Timer?

    init(runtime: Runtime) {
        self.runtime = runtime
        self.previewSkin = runtime.skinId

        let size = CGSize(width: 260, height: 320)
        super.init(
            contentRect: CGRect(origin: .zero, size: size),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
        title = "Customize Pet"
        isReleasedWhenClosed = false
        center()

        let content = NSView(frame: CGRect(origin: .zero, size: size))
        contentView = content

        preview.frame = CGRect(x: (size.width - 160) / 2, y: 130, width: 160, height: 160)
        preview.imageScaling = .scaleProportionallyUpOrDown
        content.addSubview(preview)

        skinLabel.frame = CGRect(x: 0, y: 100, width: size.width, height: 20)
        skinLabel.alignment = .center
        skinLabel.font = .boldSystemFont(ofSize: 13)
        content.addSubview(skinLabel)

        let prevButton = NSButton(title: "◀", target: self, action: #selector(previousSkin))
        prevButton.frame = CGRect(x: 30, y: 70, width: 40, height: 24)
        content.addSubview(prevButton)

        let nextButton = NSButton(title: "▶", target: self, action: #selector(nextSkin))
        nextButton.frame = CGRect(x: size.width - 70, y: 70, width: 40, height: 24)
        content.addSubview(nextButton)

        var y: CGFloat = 40
        for accessory in AccessoryId.allCases {
            let checkbox = NSButton(checkboxWithTitle: accessory.displayName, target: self, action: #selector(toggleAccessory(_:)))
            checkbox.frame = CGRect(x: 20, y: y, width: size.width - 40, height: 20)
            checkbox.state = runtime.accessoryIds.contains(accessory) ? .on : .off
            checkbox.tag = AccessoryId.allCases.firstIndex(of: accessory) ?? 0
            content.addSubview(checkbox)
            accessoryCheckboxes[accessory] = checkbox
            y -= 24
        }

        updatePreview()
    }

    func present() {
        makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        previewTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.advancePreviewFrame() }
        }
    }

    override func close() {
        previewTimer?.invalidate()
        previewTimer = nil
        super.close()
    }

    @objc private func previousSkin() {
        let all = SkinId.allCases
        let idx = all.firstIndex(of: previewSkin) ?? 0
        previewSkin = all[(idx - 1 + all.count) % all.count]
        frameIndex = 0
        applySkin()
    }

    @objc private func nextSkin() {
        let all = SkinId.allCases
        let idx = all.firstIndex(of: previewSkin) ?? 0
        previewSkin = all[(idx + 1) % all.count]
        frameIndex = 0
        applySkin()
    }

    private func applySkin() {
        runtime?.setSkin(previewSkin)
        updatePreview()
    }

    @objc private func toggleAccessory(_ sender: NSButton) {
        guard let accessory = AccessoryId.allCases[safe: sender.tag] else { return }
        runtime?.setAccessory(accessory, worn: sender.state == .on)
        updatePreview()
    }

    private func advancePreviewFrame() {
        guard let clip = Skins.all[previewSkin]?.clips[.idle] else { return }
        frameIndex = (frameIndex + 1) % clip.frames.count
        updatePreview()
    }

    private func updatePreview() {
        guard let skin = Skins.all[previewSkin], let clip = skin.clips[.idle] else { return }
        skinLabel.stringValue = previewSkin.displayName
        let idx = min(frameIndex, clip.frames.count - 1)
        let worn = AccessoryId.allCases.filter { accessoryCheckboxes[$0]?.state == .on }
        let accessories = worn.compactMap { Accessories.all[$0] }
        let image = PixelArtRenderer.renderComposite(grid: clip.frames[idx], palette: skin.palette, accessories: accessories, zoom: 10)
        let nsImage = NSImage(cgImage: image, size: NSSize(width: image.width, height: image.height))
        preview.image = nsImage
    }
}

private extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
