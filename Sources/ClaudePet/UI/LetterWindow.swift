import AppKit

/// Colors shared by the letter-styled UI - the clay tone matches the pet
/// sprite's own body color (see `Sprites.swift`) so the compose window reads
/// as part of the same character rather than a generic system dialog.
enum LetterTheme {
    static let paper = NSColor(calibratedRed: 0.965, green: 0.941, blue: 0.898, alpha: 1)
    static let paperShadowLine = NSColor(calibratedRed: 0.85, green: 0.8, blue: 0.73, alpha: 1)
    static let ink = NSColor(calibratedRed: 0.16, green: 0.12, blue: 0.09, alpha: 1)
    static let inkFaint = NSColor(calibratedRed: 0.16, green: 0.12, blue: 0.09, alpha: 0.45)
    static let clay = NSColor(calibratedRed: 0.776, green: 0.455, blue: 0.345, alpha: 1)
}

/// The card the letter's contents sit on - draws the paper fill, a clay
/// border, a wax-seal dot by the title, and a faint rule under it. Everything
/// else (text view, buttons) is a normal subview on top.
private final class LetterCardView: NSView {
    override func draw(_ dirtyRect: NSRect) {
        let card = NSBezierPath(roundedRect: bounds.insetBy(dx: 1, dy: 1), xRadius: 14, yRadius: 14)
        LetterTheme.paper.setFill()
        card.fill()
        LetterTheme.clay.withAlphaComponent(0.35).setStroke()
        card.lineWidth = 1
        card.stroke()

        let seal = NSBezierPath(ovalIn: CGRect(x: bounds.width - 34, y: bounds.height - 34, width: 14, height: 14))
        LetterTheme.clay.setFill()
        seal.fill()
    }
}

/// A pill-shaped, filled button used for the primary "Send" action - the
/// system `NSButton` bezel styles don't have anything this rounded, so it's
/// drawn with a layer instead of relying on a bezel style.
private final class PillButton: NSButton {
    var fillColor: NSColor = LetterTheme.clay {
        didSet { layer?.backgroundColor = fillColor.cgColor }
    }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        isBordered = false
        wantsLayer = true
        layer?.backgroundColor = fillColor.cgColor
        contentTintColor = .white
        font = .systemFont(ofSize: 13, weight: .semibold)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func layout() {
        super.layout()
        layer?.cornerRadius = bounds.height / 2
    }
}

/// A borderless, letter-themed replacement for the system "Send a message"
/// alert. Handles its own modal session so `MessageComposer` just needs to
/// run it and read back the result.
final class LetterWindow: NSWindow {
    private let textView = PlaceholderTextView()
    private var peerPopup: NSPopUpButton?
    private let singlePeer: String?
    private var result: (text: String, peer: String)?

    init(peerNames: [String]) {
        let size = CGSize(width: 360, height: 300)
        singlePeer = peerNames.count == 1 ? peerNames[0] : nil

        super.init(
            contentRect: CGRect(origin: .zero, size: size),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )

        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        isMovableByWindowBackground = true
        isReleasedWhenClosed = false
        level = .modalPanel

        let card = LetterCardView(frame: CGRect(origin: .zero, size: size))
        contentView = card

        let title = NSTextField(labelWithString: "Send a Letter")
        title.font = NSFont(name: "Georgia-Bold", size: 18) ?? .boldSystemFont(ofSize: 18)
        title.textColor = LetterTheme.clay
        title.frame = CGRect(x: 22, y: size.height - 46, width: size.width - 44, height: 24)
        card.addSubview(title)

        let rule = NSBox(frame: CGRect(x: 22, y: size.height - 54, width: size.width - 44, height: 1))
        rule.boxType = .custom
        rule.borderColor = LetterTheme.paperShadowLine
        rule.fillColor = LetterTheme.paperShadowLine
        card.addSubview(rule)

        var toY = size.height - 80
        let toLabel = NSTextField(labelWithString: "To:")
        toLabel.font = NSFont(name: "Georgia", size: 13) ?? .systemFont(ofSize: 13)
        toLabel.textColor = LetterTheme.inkFaint
        toLabel.frame = CGRect(x: 22, y: toY, width: 28, height: 20)
        card.addSubview(toLabel)

        if let singlePeer {
            let peerLabel = NSTextField(labelWithString: singlePeer)
            peerLabel.font = NSFont(name: "Georgia-Italic", size: 13) ?? .systemFont(ofSize: 13)
            peerLabel.textColor = LetterTheme.ink
            peerLabel.frame = CGRect(x: 48, y: toY, width: size.width - 70, height: 20)
            card.addSubview(peerLabel)
        } else {
            let popup = NSPopUpButton(frame: CGRect(x: 44, y: toY - 3, width: size.width - 66, height: 24), pullsDown: false)
            popup.addItems(withTitles: peerNames)
            popup.isBordered = false
            (popup.cell as? NSPopUpButtonCell)?.arrowPosition = .arrowAtCenter
            let font = NSFont(name: "Georgia-Italic", size: 13) ?? .systemFont(ofSize: 13)
            for item in popup.itemArray {
                item.attributedTitle = NSAttributedString(string: item.title, attributes: [.font: font, .foregroundColor: LetterTheme.ink])
            }
            card.addSubview(popup)
            peerPopup = popup
            toY -= 4
        }

        let scroll = NSScrollView(frame: CGRect(x: 20, y: 56, width: size.width - 40, height: toY - 66))
        scroll.hasVerticalScroller = true
        scroll.drawsBackground = false
        scroll.borderType = .noBorder

        textView.font = NSFont(name: "Georgia", size: 14) ?? .systemFont(ofSize: 14)
        textView.textColor = LetterTheme.ink
        textView.backgroundColor = .clear
        textView.drawsBackground = false
        textView.isRichText = false
        textView.textContainerInset = CGSize(width: 4, height: 6)
        textView.placeholderString = "What's on your mind..."
        textView.placeholderColor = LetterTheme.inkFaint
        textView.frame = CGRect(origin: .zero, size: scroll.contentSize)
        textView.autoresizingMask = [.width]
        textView.minSize = CGSize(width: 0, height: scroll.contentSize.height)
        textView.maxSize = CGSize(width: scroll.contentSize.width, height: .greatestFiniteMagnitude)
        textView.isVerticallyResizable = true

        scroll.documentView = textView
        card.addSubview(scroll)

        let cancel = NSButton(frame: CGRect(x: 20, y: 18, width: 70, height: 28))
        cancel.title = "Cancel"
        cancel.isBordered = false
        cancel.font = .systemFont(ofSize: 13)
        cancel.contentTintColor = LetterTheme.inkFaint
        cancel.target = self
        cancel.action = #selector(cancelTapped)
        cancel.keyEquivalent = "\u{1b}" // Escape
        card.addSubview(cancel)

        let send = PillButton(frame: CGRect(x: size.width - 90, y: 14, width: 70, height: 34))
        send.title = "Send"
        send.target = self
        send.action = #selector(sendTapped)
        send.keyEquivalent = "\r"
        send.keyEquivalentModifierMask = .command
        card.addSubview(send)
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }

    var firstResponderTarget: NSView { textView }

    @objc private func sendTapped() {
        let text = textView.string.trimmingCharacters(in: .whitespacesAndNewlines)
        let peer = singlePeer ?? peerPopup?.titleOfSelectedItem
        if !text.isEmpty, let peer {
            result = (text, peer)
        }
        NSApp.stopModal()
    }

    @objc private func cancelTapped() {
        result = nil
        NSApp.stopModal()
    }

    override func cancelOperation(_ sender: Any?) {
        cancelTapped()
    }

    /// Runs the modal session and returns whatever `sendTapped`/`cancelTapped`
    /// recorded, mirroring `NSAlert.runModal`'s call shape.
    func runModal() -> (text: String, peer: String)? {
        NSApp.activate(ignoringOtherApps: true)
        center()
        makeKeyAndOrderFront(nil)
        makeFirstResponder(firstResponderTarget)
        NSApp.runModal(for: self)
        orderOut(nil)
        return result
    }
}

/// A minimal placeholder-string add-on for `NSTextView`, which has no native
/// placeholder support the way `NSTextField` does.
private final class PlaceholderTextView: NSTextView {
    var placeholderString: String = ""
    var placeholderColor: NSColor = .placeholderTextColor

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        guard string.isEmpty, let font else { return }
        let attrs: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: placeholderColor]
        let inset = textContainerInset
        NSString(string: placeholderString).draw(at: CGPoint(x: inset.width + 5, y: inset.height), withAttributes: attrs)
    }

    override func didChangeText() {
        super.didChangeText()
        needsDisplay = true
    }
}
