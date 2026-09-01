import AppKit

/// Detects whether the user is currently doomscrolling Instagram Reels, so the
/// pet can go make a nuisance of itself until they stop.
///
/// Detection is deliberately layered by how invasive it is:
///  - The native Instagram Mac app is recognized by bundle identifier alone -
///    no permission needed, `NSWorkspace.frontmostApplication` is free to read.
///  - Reels *inside a browser tab* needs the tab's URL, which AppKit has no
///    permission-free API for. We ask the browser via Apple Events
///    (`NSAppleScript`), which prompts the user for one-time Automation
///    permission the first time it's actually attempted - not on launch, and
///    only if the user is in a supported browser to begin with. If that
///    permission is denied, this path silently reports "not distracted"
///    rather than nagging; the app never asks twice in the same launch.
/// `nonisolated`/`@unchecked Sendable` because it runs its (blocking) Apple
/// Event round-trip off the main actor from a detached background task - see
/// `Runtime.scheduleDistractionCheck`. Safe because its one piece of mutable
/// state (`automationDenied`) is only ever touched sequentially from that same
/// task chain, never concurrently.
nonisolated final class DistractionDetector: @unchecked Sendable {
    private static let instagramAppBundleID = "com.burbn.instagram"

    private static let browserBundleIDs: [String: String] = [
        "com.apple.Safari": "Safari",
        "com.google.Chrome": "Google Chrome",
        "com.google.Chrome.beta": "Google Chrome Beta",
        "com.brave.Browser": "Brave Browser",
        "com.microsoft.edgemac": "Microsoft Edge",
        "company.thebrowser.Browser": "Arc",
    ]

    /// Set once an Apple Events call is denied, so we stop retrying (and
    /// stop re-prompting) for the rest of this launch.
    private var automationDenied = false

    /// Synchronous and can block briefly on the Apple Event round-trip;
    /// callers should run this off the main actor.
    func currentlyDistracted() -> Bool {
        guard let app = NSWorkspace.shared.frontmostApplication,
              let bundleID = app.bundleIdentifier else { return false }

        if bundleID == Self.instagramAppBundleID { return true }

        guard !automationDenied, let appName = Self.browserBundleIDs[bundleID] else { return false }
        return activeTabIsReels(appName: appName)
    }

    private func activeTabIsReels(appName: String) -> Bool {
        let script = """
        tell application "\(appName)"
            if (count of windows) = 0 then return ""
            set frontWindow to front window
            try
                return URL of active tab of frontWindow
            on error
                try
                    return URL of current tab of frontWindow
                on error
                    return ""
                end try
            end try
        end tell
        """

        guard let appleScript = NSAppleScript(source: script) else { return false }
        var errorInfo: NSDictionary?
        let result = appleScript.executeAndReturnError(&errorInfo)

        if let errorInfo, let code = errorInfo[NSAppleScript.errorNumber] as? Int, code == -1743 {
            // -1743 = user hasn't granted Automation permission for this app.
            automationDenied = true
            return false
        }

        guard let urlString = result.stringValue else { return false }
        return urlString.contains("instagram.com/reels")
    }
}
