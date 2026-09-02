import AppKit

/// The compose panel shared by the status-bar menu and the pet's own
/// right-click menu - both routes end up here rather than each rolling their
/// own dialog.
enum MessageComposer {
    /// Presents a modal, letter-themed compose panel (see `LetterWindow`).
    /// Returns `nil` if the user cancelled. If there's exactly one nearby
    /// peer it's targeted automatically and no picker is shown; with more
    /// than one, a checklist lets the user address the letter to several at
    /// once (all checked by default).
    static func present(peerNames: [String]) -> (text: String, peers: [String])? {
        guard !peerNames.isEmpty else {
            let alert = NSAlert()
            alert.messageText = "No pets nearby"
            alert.informativeText = "Nothing responded nearby. Make sure the other pet is running and Bluetooth/Wi-Fi is on."
            alert.addButton(withTitle: "OK")
            NSApp.activate(ignoringOtherApps: true)
            alert.runModal()
            return nil
        }

        return LetterWindow(peerNames: peerNames).runModal()
    }
}
