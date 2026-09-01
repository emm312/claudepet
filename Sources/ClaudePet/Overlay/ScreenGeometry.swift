import AppKit

/// Helpers for reasoning about screen geometry in AppKit's bottom-left-origin,
/// flipped-per-display coordinate system.
enum ScreenGeometry {

    /// The screen that best contains the given point, falling back to `NSScreen.main`.
    static func screen(containing point: CGPoint) -> NSScreen? {
        NSScreen.screens.first { $0.frame.contains(point) } ?? NSScreen.main
    }

    /// Clamps a proposed origin (of a `size`-sized window) so the window stays
    /// fully on some connected screen. Used after display hot-plug / resolution
    /// changes so the pet never strands itself off in space.
    static func clampOrigin(_ origin: CGPoint, size: CGSize) -> CGPoint {
        let screens = NSScreen.screens
        guard !screens.isEmpty else { return origin }

        let proposed = CGRect(origin: origin, size: size)
        if screens.contains(where: { $0.frame.intersects(proposed) }) {
            // Still on (at least partially) a real screen - just nudge fully inside it.
            if let s = screen(containing: CGPoint(x: proposed.midX, y: proposed.midY)) {
                return clamp(origin, size: size, to: s.visibleFrame)
            }
        }

        // Fully off every screen - snap onto the main screen.
        let target = NSScreen.main?.visibleFrame ?? screens[0].visibleFrame
        return clamp(origin, size: size, to: target)
    }

    private static func clamp(_ origin: CGPoint, size: CGSize, to frame: CGRect) -> CGPoint {
        let maxX = frame.maxX - size.width
        let maxY = frame.maxY - size.height
        let x = min(max(origin.x, frame.minX), max(maxX, frame.minX))
        let y = min(max(origin.y, frame.minY), max(maxY, frame.minY))
        return CGPoint(x: x, y: y)
    }

    /// The y-coordinate of the top-most walkable line on the given screen (just under
    /// the menu bar), in AppKit's bottom-left-origin space.
    static func topWalkLine(on screen: NSScreen) -> CGFloat {
        screen.visibleFrame.maxY
    }

    /// The y-coordinate of the bottom-most walkable line (top of the Dock, or screen
    /// bottom if the Dock is hidden/absent), in AppKit's bottom-left-origin space.
    static func bottomWalkLine(on screen: NSScreen) -> CGFloat {
        screen.visibleFrame.minY
    }
}
