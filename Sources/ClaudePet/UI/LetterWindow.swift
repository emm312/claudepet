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

/// A pill-shaped, filled button used for the primary action - the system
/// `NSButton` bezel styles don't have anything this rounded, so it's drawn
/// with a layer instead of relying on a bezel style.
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

/// A borderless, letter-themed window used both to compose an outgoing
/// message and to read one that just arrived - both are "a letter in an
/// envelope", so they share one window class rather than a modal alert for
/// reading and a bespoke panel for sending. `MessageComposer` and `Runtime`
/// each just run it modally and read back the result.
final class LetterWindow: NSWindow {
    private enum Mode {
        case compose
        case read(PetMessage)
    }

    private let mode: Mode
    private let textView = PlaceholderTextView()
    /// One checkbox per peer, shown instead of `singlePeer` when there's more
    /// than one nearby - letting a letter go out to several at once.
    private var peerCheckboxes: [NSButton] = []
    private let singlePeer: String?
    private var expressCheckbox: NSButton!
    private var result: (text: String, peers: [String], express: Bool)?

    /// Only meaningful in `.read` mode: whether the reader has switched the
    /// panel over to composing a reply.
    private var isReplying = false

    private var titleLabel: NSTextField!
    private var toLabel: NSTextField!
    private var leftButton: NSButton!
    private var rightButton: PillButton!

    private static let baseSize = CGSize(width: 360, height: 300)
    private static let peerRowHeight: CGFloat = 20
    private let windowSize: CGSize

    /// Compose mode: presents a blank letter addressed to one or more of
    /// `peerNames`. The window grows to fit the peer checklist when there's
    /// more than one nearby.
    init(peerNames: [String]) {
        mode = .compose
        singlePeer = peerNames.count == 1 ? peerNames[0] : nil
        let extraRows = max(0, peerNames.count - 1)
        windowSize = CGSize(width: Self.baseSize.width, height: Self.baseSize.height + CGFloat(extraRows) * Self.peerRowHeight)
        super.init(
            contentRect: CGRect(origin: .zero, size: windowSize),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        configureWindow()
        buildUI(peerNames: peerNames)
    }

    /// Read mode: presents an already-arrived `message`, read-only until the
    /// reader taps "Reply".
    init(message: PetMessage) {
        mode = .read(message)
        singlePeer = message.senderName
        windowSize = Self.baseSize
        super.init(
            contentRect: CGRect(origin: .zero, size: windowSize),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        configureWindow()
        buildUI(peerNames: [message.senderName])
    }

    private func configureWindow() {
        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        isMovableByWindowBackground = true
        isReleasedWhenClosed = false
        level = .modalPanel
    }

    private func buildUI(peerNames: [String]) {
        let size = windowSize
        let card = LetterCardView(frame: CGRect(origin: .zero, size: size))
        contentView = card

        let title = NSTextField(labelWithString: "")
        title.font = NSFont(name: "Georgia-Bold", size: 18) ?? .boldSystemFont(ofSize: 18)
        title.textColor = LetterTheme.clay
        title.frame = CGRect(x: 22, y: size.height - 46, width: size.width - 44, height: 24)
        card.addSubview(title)
        titleLabel = title

        let rule = NSBox(frame: CGRect(x: 22, y: size.height - 54, width: size.width - 44, height: 1))
        rule.boxType = .custom
        rule.borderColor = LetterTheme.paperShadowLine
        rule.fillColor = LetterTheme.paperShadowLine
        card.addSubview(rule)

        var toY = size.height - 80
        let to = NSTextField(labelWithString: "")
        to.font = NSFont(name: "Georgia", size: 13) ?? .systemFont(ofSize: 13)
        to.textColor = LetterTheme.inkFaint
        to.frame = CGRect(x: 22, y: toY, width: 44, height: 20)
        card.addSubview(to)
        toLabel = to

        if let singlePeer {
            let peerLabel = NSTextField(labelWithString: singlePeer)
            peerLabel.font = NSFont(name: "Georgia-Italic", size: 13) ?? .systemFont(ofSize: 13)
            peerLabel.textColor = LetterTheme.ink
            peerLabel.frame = CGRect(x: 60, y: toY, width: size.width - 82, height: 20)
            card.addSubview(peerLabel)
        } else {
            // A checkbox per nearby peer, all checked by default, so one
            // letter can go out to several pets at once.
            let font = NSFont(name: "Georgia-Italic", size: 13) ?? .systemFont(ofSize: 13)
            var y = toY
            for name in peerNames {
                let checkbox = NSButton(checkboxWithTitle: name, target: nil, action: nil)
                checkbox.attributedTitle = NSAttributedString(string: name, attributes: [.font: font, .foregroundColor: LetterTheme.ink])
                checkbox.state = .on
                checkbox.frame = CGRect(x: 46, y: y - 2, width: size.width - 68, height: Self.peerRowHeight)
                card.addSubview(checkbox)
                peerCheckboxes.append(checkbox)
                y -= Self.peerRowHeight
            }
            toY = y - 2
        }

        // windows-branch feature: send this one by horse, riding faster and
        // showing the horse/mail props on both ends. See HorseSprite.swift/MailSprite.swift.
        let expressFont = NSFont(name: "Georgia-Italic", size: 12) ?? .systemFont(ofSize: 12)
        let express = NSButton(checkboxWithTitle: "Send by horse (express) \u{1F40E}", target: nil, action: nil)
        express.attributedTitle = NSAttributedString(
            string: "Send by horse (express) \u{1F40E}",
            attributes: [.font: expressFont, .foregroundColor: LetterTheme.inkFaint]
        )
        express.frame = CGRect(x: 20, y: toY - 24, width: size.width - 40, height: Self.peerRowHeight)
        card.addSubview(express)
        expressCheckbox = express
        toY -= 26

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
        textView.placeholderColor = LetterTheme.inkFaint
        textView.frame = CGRect(origin: .zero, size: scroll.contentSize)
        textView.autoresizingMask = [.width]
        textView.minSize = CGSize(width: 0, height: scroll.contentSize.height)
        textView.maxSize = CGSize(width: scroll.contentSize.width, height: .greatestFiniteMagnitude)
        textView.isVerticallyResizable = true

        scroll.documentView = textView
        card.addSubview(scroll)

        let left = NSButton(frame: CGRect(x: 20, y: 18, width: 70, height: 28))
        left.isBordered = false
        left.font = .systemFont(ofSize: 13)
        left.contentTintColor = LetterTheme.inkFaint
        left.target = self
        left.action = #selector(leftTapped)
        left.keyEquivalent = "\u{1b}" // Escape
        card.addSubview(left)
        leftButton = left

        let right = PillButton(frame: CGRect(x: size.width - 90, y: 14, width: 70, height: 34))
        right.target = self
        right.action = #selector(rightTapped)
        right.keyEquivalent = "\r"
        right.keyEquivalentModifierMask = .command
        card.addSubview(right)
        rightButton = right

        switch mode {
        case .compose:
            textView.isEditable = true
            textView.placeholderString = "What's on your mind..."
        case .read(let message):
            textView.string = message.text
            textView.isEditable = false
        }
        refreshLabels()
    }

    private func refreshLabels() {
        switch mode {
        case .compose:
            titleLabel.stringValue = "Send a Letter"
            toLabel.stringValue = "To:"
            leftButton.title = "Cancel"
            rightButton.title = "Send"
        case .read(let message):
            if isReplying {
                titleLabel.stringValue = "Send a Reply"
                toLabel.stringValue = "To:"
                leftButton.title = "Cancel"
                rightButton.title = "Send"
            } else {
                titleLabel.stringValue = "A Letter Arrived"
                toLabel.stringValue = "From:"
                leftButton.title = "OK"
                rightButton.title = "Reply"
                _ = message // sender name is rendered by the peer label above
            }
        }
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }

    var firstResponderTarget: NSView { textView }

    @objc private func leftTapped() {
        switch mode {
        case .compose:
            result = nil
            NSApp.stopModal()
        case .read:
            if isReplying {
                // Back out of composing a reply, restoring the original letter.
                isReplying = false
                if case .read(let message) = mode { textView.string = message.text }
                textView.isEditable = false
                refreshLabels()
            } else {
                result = nil
                NSApp.stopModal()
            }
        }
    }

    @objc private func rightTapped() {
        switch mode {
        case .compose:
            submitIfNonEmpty()
        case .read:
            if isReplying {
                submitIfNonEmpty()
            } else {
                isReplying = true
                textView.string = ""
                textView.isEditable = true
                textView.placeholderString = "Your reply..."
                refreshLabels()
                makeFirstResponder(firstResponderTarget)
            }
        }
    }

    private func submitIfNonEmpty() {
        let text = textView.string.trimmingCharacters(in: .whitespacesAndNewlines)
        let peers = singlePeer.map { [$0] } ?? peerCheckboxes.filter { $0.state == .on }.map(\.title)
        if !text.isEmpty, !peers.isEmpty {
            result = (text, peers, expressCheckbox.state == .on)
            NSApp.stopModal()
        }
    }

    override func cancelOperation(_ sender: Any?) {
        leftTapped()
    }

    /// Runs the modal session and returns whatever the reader/sender decided,
    /// mirroring `NSAlert.runModal`'s call shape. `nil` means "closed without
    /// sending" - either a cancelled compose, or a read letter dismissed with
    /// "OK" and no reply.
    func runModal() -> (text: String, peers: [String], express: Bool)? {
        NSApp.activate(ignoringOtherApps: true)
        center()
        makeKeyAndOrderFront(nil)
        if case .compose = mode { makeFirstResponder(firstResponderTarget) }
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
