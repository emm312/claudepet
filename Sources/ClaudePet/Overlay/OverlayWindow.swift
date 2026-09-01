import AppKit

/// A borderless, transparent, always-on-top window that is clipped tightly to the
/// pet's sprite bounding box - never a full-screen sheet. It lives above normal
/// windows and fullscreen apps, follows the user across every Space, but stays
/// below the screensaver / lock screen shielding level so it never draws over
/// those.
final class OverlayWindow: NSWindow {

    init(size: CGSize) {
        super.init(
            contentRect: CGRect(origin: .zero, size: size),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )

        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        isMovableByWindowBackground = false
        isMovable = false
        ignoresMouseEvents = true // flipped on/off per-pixel by the hosted view
        isReleasedWhenClosed = false
        collectionBehavior = [
            .canJoinAllSpaces,
            .fullScreenAuxiliary,
            .stationary,
            .ignoresCycle
        ]
        // Sit just below the screensaver/lock-screen shielding level so we're
        // above fullscreen apps and normal windows, but never over the lock screen.
        level = NSWindow.Level(rawValue: Int(CGWindowLevelForKey(.maximumWindow)) - 1)
        // Never let this window become key/main; it must never take focus away
        // from whatever app the user is actually using.
        collectionBehavior.insert(.transient)
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}
