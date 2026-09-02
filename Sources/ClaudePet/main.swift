import AppKit
import Foundation

// `ClaudePet --export-sprites` rasterizes every pixel-grid clip in PetSprites to
// real PNG files in Resources/sprites/ and exits - a regenerable asset pipeline
// rather than a one-off. Run this before `swift build`/bundling if the art changes.
if CommandLine.arguments.contains("--export-sprites") {
    let outputDir = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
        .appendingPathComponent("Resources/sprites")
    do {
        let written = try SpriteExporter.exportAll(to: outputDir)
        print("Exported \(written.count) sprite frames to \(outputDir.path):")
        for url in written { print("  \(url.lastPathComponent)") }
    } catch {
        print("Sprite export failed: \(error)")
        exit(1)
    }
    exit(0)
}

// `ClaudePet --debug-distraction` traces the Reels-detection flow step by step
// against whatever's actually frontmost, then exits - for diagnosing why the
// rampage isn't triggering without guessing. Focus your browser before/during
// the countdown.
let debugFlags = ["--debug-distraction", "--debug-ax-tree", "--debug-ax-windows"]
if debugFlags.contains(where: CommandLine.arguments.contains) {
    let detector = DistractionDetector()
    for remaining in stride(from: 3, through: 1, by: -1) {
        print("Checking in \(remaining)... (make sure your browser is frontmost)")
        Thread.sleep(forTimeInterval: 1)
    }
    print("---")
    if CommandLine.arguments.contains("--debug-ax-tree") {
        print(detector.debugDumpTree())
    } else if CommandLine.arguments.contains("--debug-ax-windows") {
        print(detector.debugListWindows())
    } else {
        print(detector.debugSighting())
    }
    exit(0)
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var runtime: Runtime?

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Accessory: no Dock icon, no app menu bar - only the status item.
        NSApp.setActivationPolicy(.accessory)
        runtime = Runtime()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.run()
