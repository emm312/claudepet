import AppKit
import ApplicationServices

/// The pet's only real UI surface: a menu bar item with stats, quick actions,
/// and settings. Everything else on screen is a chromeless overlay.
final class StatusItemController: NSObject, NSMenuDelegate {
    private let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    private weak var runtime: Runtime?

    private let hungerItem = NSMenuItem(title: "Hunger: --", action: nil, keyEquivalent: "")
    private let energyItem = NSMenuItem(title: "Energy: --", action: nil, keyEquivalent: "")
    private let happinessItem = NSMenuItem(title: "Happiness: --", action: nil, keyEquivalent: "")
    private let cleanlinessItem = NSMenuItem(title: "Cleanliness: --", action: nil, keyEquivalent: "")
    private var peersMenuItem: NSMenuItem!
    private var accessibilityItem: NSMenuItem!

    init(runtime: Runtime) {
        self.runtime = runtime
        super.init()

        item.button?.title = "🐾"

        let menu = NSMenu()
        menu.delegate = self

        for statItem in [hungerItem, energyItem, happinessItem, cleanlinessItem] {
            statItem.isEnabled = false
            menu.addItem(statItem)
        }
        menu.addItem(.separator())

        menu.addItem(withAction("Feed", #selector(feed)))
        menu.addItem(withAction("Play", #selector(play)))
        menu.addItem(withAction("Clean", #selector(clean)))
        menu.addItem(.separator())

        menu.addItem(withAction("Send Message…", #selector(sendMessage)))
        let peersMenuItem = withAction("Peers", nil)
        peersMenuItem.submenu = NSMenu()
        menu.addItem(peersMenuItem)
        self.peersMenuItem = peersMenuItem
        menu.addItem(.separator())

        // Only relevant while distraction detection can't work yet - hidden
        // once granted, rather than left around as a stale no-op entry.
        accessibilityItem = withAction("Grant Accessibility Access…", #selector(openAccessibilitySettings))
        menu.addItem(accessibilityItem)
        menu.addItem(.separator())

        let launchAtLoginItem = withAction("Launch at Login", #selector(toggleLaunchAtLogin))
        launchAtLoginItem.state = LoginItemManager.isEnabled ? .on : .off
        menu.addItem(launchAtLoginItem)
        menu.addItem(.separator())

        menu.addItem(withAction("Quit ClaudePet", #selector(quit)))

        item.menu = menu
        peersDidChange()
        accessibilityItem.isHidden = AXIsProcessTrusted()
    }

    private func withAction(_ title: String, _ selector: Selector?) -> NSMenuItem {
        let menuItem = NSMenuItem(title: title, action: selector, keyEquivalent: "")
        menuItem.target = self
        return menuItem
    }

    func menuWillOpen(_ menu: NSMenu) {
        guard let state = runtime?.state else { return }
        hungerItem.title = "Hunger: \(Int(state.hunger))%"
        energyItem.title = "Energy: \(Int(state.energy))%"
        happinessItem.title = "Happiness: \(Int(state.happiness))%"
        cleanlinessItem.title = "Cleanliness: \(Int(state.cleanliness))%"
        accessibilityItem.isHidden = AXIsProcessTrusted()
    }

    /// Called by the Runtime whenever the set of connected peers changes, so
    /// the "Peers" submenu stays current without needing to be reopened.
    func peersDidChange() {
        guard let peersMenuItem else { return }
        let names = runtime?.peerNames ?? []
        let submenu = NSMenu()
        if names.isEmpty {
            let empty = NSMenuItem(title: "No pets nearby", action: nil, keyEquivalent: "")
            empty.isEnabled = false
            submenu.addItem(empty)
        } else {
            for name in names {
                let item = NSMenuItem(title: name, action: nil, keyEquivalent: "")
                item.isEnabled = false
                submenu.addItem(item)
            }
        }
        peersMenuItem.submenu = submenu
    }

    @objc private func feed() { runtime?.feed() }
    @objc private func play() { runtime?.play() }
    @objc private func clean() { runtime?.clean() }
    @objc private func quit() { NSApp.terminate(nil) }
    @objc private func sendMessage() { runtime?.presentMessageComposer() }

    @objc private func toggleLaunchAtLogin(_ sender: NSMenuItem) {
        let newValue = !LoginItemManager.isEnabled
        LoginItemManager.setEnabled(newValue)
        sender.state = newValue ? .on : .off
    }

    /// Opens System Settings' Accessibility pane directly, so granting access
    /// doesn't require the user to go hunting for it themselves. This never
    /// prompts on its own - `DistractionDetector` does that (at most once per
    /// launch) the first time a browser is frontmost.
    @objc private func openAccessibilitySettings() {
        guard let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        else { return }
        NSWorkspace.shared.open(url)
    }
}
