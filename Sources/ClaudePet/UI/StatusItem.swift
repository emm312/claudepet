import AppKit

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
    /// Shown only while a delivered letter is waiting - see `menuWillOpen`.
    private var readLetterItem: NSMenuItem!
    /// Shown only once an update has been downloaded and staged - see `menuWillOpen`.
    private var installUpdateItem: NSMenuItem!
    private var autoUpdateItem: NSMenuItem!

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

        readLetterItem = withAction("Read Letter…", #selector(readLetter))
        readLetterItem.isHidden = true
        menu.addItem(readLetterItem)

        menu.addItem(withAction("Customize Pet…", #selector(customizePet)))
        menu.addItem(withAction("Send Message…", #selector(sendMessage)))
        let peersMenuItem = withAction("Peers", nil)
        peersMenuItem.submenu = NSMenu()
        menu.addItem(peersMenuItem)
        self.peersMenuItem = peersMenuItem
        menu.addItem(.separator())

        let launchAtLoginItem = withAction("Launch at Login", #selector(toggleLaunchAtLogin))
        launchAtLoginItem.state = LoginItemManager.isEnabled ? .on : .off
        menu.addItem(launchAtLoginItem)

        let autoUpdateItem = withAction("Automatic updates", #selector(toggleAutoUpdates))
        autoUpdateItem.state = (runtime.autoUpdatesEnabled) ? .on : .off
        menu.addItem(autoUpdateItem)
        self.autoUpdateItem = autoUpdateItem

        installUpdateItem = withAction("Install update…", #selector(installUpdateNow))
        installUpdateItem.isHidden = true
        menu.addItem(installUpdateItem)
        menu.addItem(.separator())

        menu.addItem(withAction("Quit ClaudePet", #selector(quit)))

        item.menu = menu
        peersDidChange()
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
        readLetterItem.isHidden = !(runtime?.hasUnreadLetter ?? false)
        updateAvailabilityChanged()
    }

    /// Called whenever a staged update appears (or the menu is about to open)
    /// so "Install update…" reflects the current pending version.
    func updateAvailabilityChanged() {
        guard let installUpdateItem else { return }
        if let version = runtime?.pendingUpdate?.version {
            installUpdateItem.title = "Install update \(version) now"
            installUpdateItem.isHidden = false
        } else {
            installUpdateItem.isHidden = true
        }
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
    @objc private func customizePet() { runtime?.presentCustomizeWindow() }
    @objc private func readLetter() { runtime?.openUnreadLetter() }

    @objc private func toggleLaunchAtLogin(_ sender: NSMenuItem) {
        let newValue = !LoginItemManager.isEnabled
        LoginItemManager.setEnabled(newValue)
        sender.state = newValue ? .on : .off
    }

    @objc private func toggleAutoUpdates(_ sender: NSMenuItem) {
        guard let runtime else { return }
        let newValue = !runtime.autoUpdatesEnabled
        runtime.setAutoUpdatesEnabled(newValue)
        sender.state = newValue ? .on : .off
    }

    /// Applies immediately, no delay/bubble - unlike the auto-apply path,
    /// which waits 8s after announcing itself (see `Runtime.performUpdateCheck`).
    @objc private func installUpdateNow() { runtime?.applyPendingUpdateNow() }
}
