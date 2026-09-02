import AppKit

/// A horizontal surface the pet can stand/walk on: the top edge of some window,
/// or the floor of a screen (menu-bar line or dock line).
struct Ledge {
    let minX: CGFloat
    let maxX: CGFloat
    /// Y in AppKit bottom-left-origin coordinates.
    let y: CGFloat
}

/// Builds the current list of walkable ledges from on-screen windows.
///
/// Deliberately reads only `kCGWindowBounds` / `kCGWindowLayer` / `kCGWindowOwnerPID`
/// from `CGWindowListCopyWindowInfo` - never `kCGWindowName` - so this needs **no**
/// Screen Recording permission and never prompts the user.
enum WindowLedges {

    /// `kCGWindowBounds` reports many windows' backing-store frame, which on
    /// modern macOS includes a few points of invisible shadow/blur padding
    /// above the actually-visible title bar - without this inset the pet
    /// stands floating on that padding, i.e. "on the shadow" instead of on the
    /// window. There's no permission-free way to get the exact visible frame
    /// (that needs Accessibility), so this is a tuned constant, not a precise
    /// fix - it lands the pet close to the real edge for the common case.
    private static let shadowInset: CGFloat = 10

    private struct WindowRect {
        let minX: CGFloat
        let maxX: CGFloat
        let topY: CGFloat
        let bottomY: CGFloat
    }

    /// Refreshed on a slow timer by the caller; this call itself is synchronous
    /// and fast (a single CG call + array filter + small O(n^2) clip pass over
    /// on-screen windows, which is at most a few dozen), safe to call a few
    /// times/sec.
    static func currentLedges(minWidth: CGFloat = 24) -> [Ledge] {
        guard let infoList = CGWindowListCopyWindowInfo(
            [.optionOnScreenOnly, .excludeDesktopElements],
            kCGNullWindowID
        ) as? [[String: AnyObject]], !NSScreen.screens.isEmpty
        else {
            return fallbackLedges()
        }

        // CGWindowListCopyWindowInfo returns windows front-to-back; we rely on
        // that order below to know which windows can occlude which.
        var windows: [WindowRect] = []
        for info in infoList {
            guard let layer = info[kCGWindowLayer as String] as? Int, layer == 0,
                  let boundsDict = info[kCGWindowBounds as String] as? [String: CGFloat],
                  let cgBounds = CGRect(dictionaryRepresentation: boundsDict as CFDictionary),
                  cgBounds.width >= minWidth, cgBounds.height >= 10,
                  // Top-left-origin CG coords -> AppKit's bottom-left-origin space.
                  let appKitBounds = ScreenGeometry.appKitRect(fromTopLeft: cgBounds)
            else { continue }

            let topY = appKitBounds.maxY - shadowInset
            let bottomY = appKitBounds.minY
            windows.append(WindowRect(minX: cgBounds.minX, maxX: cgBounds.maxX, topY: topY, bottomY: bottomY))
        }

        var ledges: [Ledge] = []
        for (i, window) in windows.enumerated() {
            var intervals: [(CGFloat, CGFloat)] = [(window.minX, window.maxX)]

            // Only windows *in front* of this one (earlier in front-to-back
            // order) can hide part of its top edge - a window behind another
            // window's ledge shouldn't offer a walkable surface where it's
            // actually covered.
            for j in 0..<i {
                let front = windows[j]
                let coversThisHeight = front.bottomY <= window.topY && window.topY <= front.topY
                let overlapsX = front.maxX > window.minX && front.minX < window.maxX
                guard coversThisHeight, overlapsX else { continue }
                intervals = subtract(intervals, (front.minX, front.maxX))
            }

            for (lo, hi) in intervals where hi - lo >= minWidth {
                ledges.append(Ledge(minX: lo, maxX: hi, y: window.topY))
            }
        }

        return ledges + fallbackLedges()
    }

    /// Removes `cut` from every interval in `intervals`, splitting an interval
    /// into two when the cut falls in its middle.
    private static func subtract(
        _ intervals: [(CGFloat, CGFloat)], _ cut: (CGFloat, CGFloat)
    ) -> [(CGFloat, CGFloat)] {
        var result: [(CGFloat, CGFloat)] = []
        for (lo, hi) in intervals {
            if cut.1 <= lo || cut.0 >= hi {
                result.append((lo, hi)) // no overlap
                continue
            }
            if cut.0 > lo { result.append((lo, cut.0)) }
            if cut.1 < hi { result.append((cut.1, hi)) }
        }
        return result
    }

    /// The floor of every screen (top of Dock / bottom of visible frame) is
    /// always walkable, so the pet always has somewhere to land.
    private static func fallbackLedges() -> [Ledge] {
        NSScreen.screens.map { screen in
            Ledge(minX: screen.frame.minX, maxX: screen.frame.maxX, y: screen.visibleFrame.minY)
        }
    }

    /// Finds the highest ledge at or below `y` that spans `x`, i.e. what the pet
    /// would land on if it fell straight down from (x, y).
    static func ledgeBelow(x: CGFloat, y: CGFloat, in ledges: [Ledge]) -> Ledge? {
        ledges
            .filter { $0.minX <= x && x <= $0.maxX && $0.y <= y }
            .max { $0.y < $1.y }
    }
}
