import AppKit
import ApplicationServices

/// A sighting of Instagram Reels open in the frontmost browser.
struct DistractionSighting {
    /// The browser's focused window, in AppKit bottom-left-origin coordinates.
    /// `nil` if the URL matched but the window geometry couldn't be read - the
    /// caller falls back to the normal ledge-bound angry sprint in that case.
    let frame: CGRect?
}

/// Detects whether the user is currently on `instagram.com/reels` in a
/// supported browser, so the pet can go rampage across that window until they
/// leave.
///
/// Detection is Accessibility-only, not Apple Events:
///  - A window *title* can't tell reels apart from the rest of Instagram
///    (browsers title both just "Instagram", or "<user> on Instagram: ...") -
///    so this reads the actual page URL off the browser's `AXWebArea` element.
///  - That one Accessibility permission also hands back the focused window's
///    frame for free, which is exactly the rectangle the rampage needs to
///    scurry around inside.
///  - No Automation (`NSAppleEventsUsageDescription`) prompt, no per-browser
///    AppleScript dialect, and no Screen Recording prompt (this never reads
///    `kCGWindowName`, matching `WindowLedges`'s existing policy).
/// `nonisolated`/`@unchecked Sendable` because it runs its Accessibility calls
/// off the main actor from a detached background task - see
/// `Runtime.scheduleDistractionCheck`. Safe because its one piece of mutable
/// state (`hasPromptedForAccess`) is only ever touched sequentially from that
/// same task chain, never concurrently.
nonisolated final class DistractionDetector: @unchecked Sendable {
    private static let browserBundleIDs: Set<String> = [
        "com.apple.Safari",
        "com.google.Chrome",
        "com.google.Chrome.beta",
        "com.brave.Browser",
        "com.microsoft.edgemac",
        "company.thebrowser.Browser", // Arc
        "org.mozilla.firefox",
        "com.vivaldi.Vivaldi",
    ]

    /// Set once we've asked the user to grant Accessibility access, so we
    /// never re-prompt within the same launch - `AXIsProcessTrusted()` is
    /// still checked fresh on every poll, so a grant made mid-session (via
    /// System Settings or the status-item shortcut) takes effect immediately.
    private var hasPromptedForAccess = false

    /// Synchronous and can block briefly on Accessibility IPC; callers should
    /// run this off the main actor.
    func currentSighting() -> DistractionSighting? {
        guard let app = NSWorkspace.shared.frontmostApplication,
              let bundleID = app.bundleIdentifier,
              Self.browserBundleIDs.contains(bundleID)
        else { return nil }

        guard ensureAccessibilityAccess() else { return nil }

        let axApp = AXUIElementCreateApplication(app.processIdentifier)
        Self.enableEnhancedAccessibility(for: axApp)
        guard let focusedWindow = Self.copyElement(axApp, kAXFocusedWindowAttribute) else { return nil }

        guard let url = Self.findWebAreaURL(in: focusedWindow), Self.urlIsReels(url) else { return nil }

        return DistractionSighting(frame: Self.frame(of: focusedWindow))
    }

    /// Chromium (and forks - Chrome, Edge, Brave, and apparently Arc) doesn't
    /// build out its full accessibility tree until something signals that an
    /// assistive-technology client is present; VoiceOver triggers this
    /// automatically, but a plain `AXUIElementCopyAttributeValue` poll like
    /// ours doesn't. Setting this undocumented-but-widely-relied-on attribute
    /// on the app element forces it on. Idempotent and cheap, safe to call on
    /// every poll; a no-op (silently ignored) on non-Chromium browsers like
    /// Safari, which don't define the attribute at all.
    private static func enableEnhancedAccessibility(for axApp: AXUIElement) {
        AXUIElementSetAttributeValue(axApp, "AXManualAccessibility" as CFString, kCFBooleanTrue)
    }

    /// Checks (without prompting) whether we're trusted; prompts at most once
    /// per launch, and only once a browser is actually frontmost, so there's
    /// no surprise dialog at app launch for users who never open a browser.
    private func ensureAccessibilityAccess() -> Bool {
        if AXIsProcessTrusted() { return true }
        guard !hasPromptedForAccess else { return false }
        hasPromptedForAccess = true
        let options: NSDictionary = ["AXTrustedCheckOptionPrompt": true]
        return AXIsProcessTrustedWithOptions(options)
    }

    // MARK: - Debugging

    /// Runs the same detection flow as `currentSighting()` but returns a
    /// verbose, human-readable trace of every step instead of just the
    /// result - for `ClaudePet --debug-distraction` to print. Never used in
    /// the normal polling path.
    func debugSighting() -> String {
        var lines: [String] = []

        guard let app = NSWorkspace.shared.frontmostApplication else {
            return "frontmost application: none"
        }
        let bundleID = app.bundleIdentifier ?? "(no bundle id)"
        lines.append("frontmost application: \(app.localizedName ?? "?") [\(bundleID)]")

        guard Self.browserBundleIDs.contains(bundleID) else {
            lines.append("-> not in the browser allowlist, stopping here")
            return lines.joined(separator: "\n")
        }
        lines.append("-> in the browser allowlist")

        let trusted = AXIsProcessTrusted()
        lines.append("AXIsProcessTrusted(): \(trusted)")
        guard ensureAccessibilityAccess() else {
            lines.append("-> not trusted (and either already prompted this launch, or the prompt was just shown/denied)")
            return lines.joined(separator: "\n")
        }

        let axApp = AXUIElementCreateApplication(app.processIdentifier)
        Self.enableEnhancedAccessibility(for: axApp)
        guard let focusedWindow = Self.copyElement(axApp, kAXFocusedWindowAttribute) else {
            lines.append("-> could not read kAXFocusedWindowAttribute (no focused window, or AX call failed)")
            return lines.joined(separator: "\n")
        }
        lines.append("-> got focused window")

        if let frame = Self.frame(of: focusedWindow) {
            lines.append("-> window frame (AppKit coords): \(frame)")
        } else {
            lines.append("-> could not read window position/size")
        }

        guard let url = Self.findWebAreaURL(in: focusedWindow) else {
            lines.append("-> no AXWebArea/AXURL found by walking the window's AX children (bounded to depth \(Self.maxWalkDepth), \(Self.maxWalkNodes) nodes)")
            return lines.joined(separator: "\n")
        }
        lines.append("-> found page URL: \(url.absoluteString)")
        lines.append("urlIsReels: \(Self.urlIsReels(url))")
        return lines.joined(separator: "\n")
    }

    /// Lists every top-level AXWindow of the frontmost app (not just the
    /// focused one) with its role/subrole/title and whether an AXWebArea is
    /// reachable inside it - some browsers (seemingly Arc) put the actual web
    /// content in a window that isn't the one `kAXFocusedWindowAttribute`
    /// reports. Temporary diagnostic for `ClaudePet --debug-ax-windows`.
    func debugListWindows() -> String {
        guard let app = NSWorkspace.shared.frontmostApplication else { return "no frontmost application" }
        let axApp = AXUIElementCreateApplication(app.processIdentifier)
        Self.enableEnhancedAccessibility(for: axApp)
        guard let windowsValue = Self.copyValue(axApp, kAXWindowsAttribute),
              let windows = windowsValue as? [AXUIElement]
        else { return "could not read kAXWindowsAttribute" }

        var lines = ["\(windows.count) window(s):"]
        for (i, window) in windows.enumerated() {
            let role = Self.copyString(window, kAXRoleAttribute) ?? "?"
            let subrole = Self.copyString(window, kAXSubroleAttribute) ?? "-"
            let title = Self.copyString(window, kAXTitleAttribute) ?? "(no title)"
            let hasWebArea = Self.findWebAreaURL(in: window, maxDepth: 20, maxNodes: 5000) != nil
            lines.append("[\(i)] \(role)/\(subrole) title=\"\(title)\" webAreaFound=\(hasWebArea)")
        }
        return lines.joined(separator: "\n")
    }

    /// Dumps the frontmost browser's focused-window AX tree (role + child
    /// count per node, indented by depth) so an unfamiliar browser's actual
    /// structure can be read off directly instead of guessed at. Temporary
    /// diagnostic for `ClaudePet --debug-ax-tree` - no bound on depth/nodes,
    /// since the whole point is to see what the bounded real walk is missing.
    func debugDumpTree(maxDepth: Int = 12) -> String {
        guard let app = NSWorkspace.shared.frontmostApplication else { return "no frontmost application" }
        let axApp = AXUIElementCreateApplication(app.processIdentifier)
        Self.enableEnhancedAccessibility(for: axApp)
        guard let focusedWindow = Self.copyElement(axApp, kAXFocusedWindowAttribute) else {
            return "no focused window"
        }
        var lines: [String] = []
        var visited = 0
        func walk(_ element: AXUIElement, depth: Int) {
            visited += 1
            guard visited <= 2000, depth <= maxDepth else { return }
            let role = Self.copyString(element, kAXRoleAttribute) ?? "?"
            let subrole = Self.copyString(element, kAXSubroleAttribute)
            let children = Self.copyChildren(element) ?? []
            let roleDesc = subrole.map { "\(role)/\($0)" } ?? role
            lines.append(String(repeating: "  ", count: depth) + roleDesc + " (\(children.count) children)")
            for child in children { walk(child, depth: depth + 1) }
        }
        walk(focusedWindow, depth: 0)
        return lines.joined(separator: "\n")
    }

    // MARK: - URL matching

    /// True iff `url`'s host is instagram.com (or a subdomain of it) and its
    /// path is the reels feed - not the home feed, not DMs, and not a single
    /// shared reel (`/reel/<id>`, singular), only the infinite `/reels` feed.
    static func urlIsReels(_ url: URL) -> Bool {
        guard let host = url.host?.lowercased() else { return false }
        let isInstagramHost = host == "instagram.com" || host.hasSuffix(".instagram.com")
        guard isInstagramHost else { return false }

        let path = url.path
        return path == "/reels" || path.hasPrefix("/reels/")
    }

    // MARK: - Accessibility plumbing

    /// Breadth-first search for the browser's web-content element, bounded so
    /// a pathological AX tree can't stall the poller.
    private static let maxWalkDepth = 6
    private static let maxWalkNodes = 200

    private static func findWebAreaURL(
        in root: AXUIElement, maxDepth: Int = maxWalkDepth, maxNodes: Int = maxWalkNodes
    ) -> URL? {
        var queue: [(AXUIElement, Int)] = [(root, 0)]
        var visited = 0

        while !queue.isEmpty {
            let (element, depth) = queue.removeFirst()
            visited += 1
            if visited > maxNodes { return nil }

            if copyString(element, kAXRoleAttribute) == "AXWebArea" {
                guard let urlValue = copyValue(element, "AXURL") else { return nil }
                if let url = urlValue as? URL { return url }
                if let urlString = urlValue as? String { return URL(string: urlString) }
                return nil
            }

            guard depth < maxDepth, let children = copyChildren(element) else { continue }
            queue.append(contentsOf: children.map { ($0, depth + 1) })
        }
        return nil
    }

    private static func frame(of window: AXUIElement) -> CGRect? {
        guard let position = copyPoint(window, kAXPositionAttribute),
              let size = copySize(window, kAXSizeAttribute),
              let appKitRect = ScreenGeometry.appKitRect(fromTopLeft: CGRect(origin: position, size: size))
        else { return nil }
        return appKitRect
    }

    private static func copyElement(_ element: AXUIElement, _ attribute: String) -> AXUIElement? {
        var value: AnyObject?
        guard AXUIElementCopyAttributeValue(element, attribute as CFString, &value) == .success else { return nil }
        return (value as! AXUIElement?)
    }

    private static func copyValue(_ element: AXUIElement, _ attribute: String) -> AnyObject? {
        var value: AnyObject?
        guard AXUIElementCopyAttributeValue(element, attribute as CFString, &value) == .success else { return nil }
        return value
    }

    private static func copyString(_ element: AXUIElement, _ attribute: String) -> String? {
        copyValue(element, attribute) as? String
    }

    private static func copyChildren(_ element: AXUIElement) -> [AXUIElement]? {
        guard let value = copyValue(element, kAXChildrenAttribute) else { return nil }
        return (value as? [AXUIElement])
    }

    private static func copyPoint(_ element: AXUIElement, _ attribute: String) -> CGPoint? {
        guard let value = copyValue(element, attribute) else { return nil }
        var point = CGPoint.zero
        guard AXValueGetType(value as! AXValue) == .cgPoint,
              AXValueGetValue(value as! AXValue, .cgPoint, &point)
        else { return nil }
        return point
    }

    private static func copySize(_ element: AXUIElement, _ attribute: String) -> CGSize? {
        guard let value = copyValue(element, attribute) else { return nil }
        var size = CGSize.zero
        guard AXValueGetType(value as! AXValue) == .cgSize,
              AXValueGetValue(value as! AXValue, .cgSize, &size)
        else { return nil }
        return size
    }
}
