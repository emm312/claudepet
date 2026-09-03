import AppKit
import CryptoKit

/// GitHub-Releases-based auto-updater. Mirrors `src-win/src/update.rs`'s
/// check/download/verify/apply flow (WinHTTP + BCrypt there, URLSession +
/// CryptoKit here) so both platforms read the same `emm312/claudepet`
/// releases and asset-naming convention. Zero external dependencies, like
/// the rest of this app.
enum Updater {
    private static let repo = "emm312/claudepet"
    private static let zipAssetName = "ClaudePet-mac.zip"
    private static let sha256AssetName = "ClaudePet-mac.zip.sha256"
    /// Sanity floor so a truncated/proxy-mangled download doesn't get staged.
    private static let minZipBytes = 100_000

    struct UpdateInfo {
        let version: String
        let zipURL: URL
        let sha256URL: URL?
    }

    private struct GitHubRelease: Codable {
        let tag_name: String
        let assets: [Asset]
    }

    private struct Asset: Codable {
        let name: String
        let browser_download_url: URL
    }

    static var currentVersion: String {
        (Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String) ?? "0.0.0"
    }

    /// Only a real `/Applications` install self-replaces, mirroring the
    /// Windows check for a sibling `uninstall.exe` - a `swift build`/dev run
    /// never overwrites itself.
    static var isRealInstall: Bool {
        Bundle.main.bundlePath.hasPrefix("/Applications/")
    }

    /// Fetches the latest release and returns update info if it's newer than
    /// the running app. Returns `nil` on any network/parse failure, no
    /// release yet, or if it's not newer.
    static func check() async -> UpdateInfo? {
        guard let url = URL(string: "https://api.github.com/repos/\(repo)/releases/latest") else { return nil }
        var request = URLRequest(url: url)
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        guard let (data, response) = try? await URLSession.shared.data(for: request),
              let http = response as? HTTPURLResponse, http.statusCode == 200,
              let release = try? JSONDecoder().decode(GitHubRelease.self, from: data)
        else { return nil }

        guard isNewer(release.tag_name, than: currentVersion) else { return nil }
        guard let zipAsset = release.assets.first(where: { $0.name == zipAssetName }) else { return nil }
        let sha256URL = release.assets.first(where: { $0.name == sha256AssetName })?.browser_download_url

        return UpdateInfo(version: release.tag_name, zipURL: zipAsset.browser_download_url, sha256URL: sha256URL)
    }

    /// Downloads and verifies the release zip, unzips it, and clears the
    /// downloaded app's quarantine flag (it's only ad-hoc/dev signed, not
    /// notarized). Returns the staged `.app` URL.
    static func downloadAndStage(_ info: UpdateInfo) async throws -> URL {
        let (zipData, zipResponse) = try await URLSession.shared.data(from: info.zipURL)
        guard let http = zipResponse as? HTTPURLResponse, http.statusCode == 200, zipData.count >= minZipBytes else {
            throw UpdaterError.downloadFailed
        }

        if let sha256URL = info.sha256URL {
            let (shaData, shaResponse) = try await URLSession.shared.data(from: sha256URL)
            guard let shaHTTP = shaResponse as? HTTPURLResponse, shaHTTP.statusCode == 200 else {
                throw UpdaterError.downloadFailed
            }
            let expected = String(data: shaData, encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
            let actual = SHA256.hash(data: zipData).map { String(format: "%02x", $0) }.joined()
            guard expected == actual else { throw UpdaterError.checksumMismatch }
        }

        let stageDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("ClaudePet-update-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: stageDir, withIntermediateDirectories: true)
        let zipPath = stageDir.appendingPathComponent(zipAssetName)
        try zipData.write(to: zipPath)

        try run("/usr/bin/ditto", ["-x", "-k", zipPath.path, stageDir.path])
        let appPath = stageDir.appendingPathComponent("ClaudePet.app")
        guard FileManager.default.fileExists(atPath: appPath.path) else { throw UpdaterError.unzipFailed }

        try? run("/usr/bin/xattr", ["-dr", "com.apple.quarantine", appPath.path])
        return appPath
    }

    /// Replaces the running app with `staged` and relaunches it, then exits.
    /// Never returns on success.
    static func applyAndRelaunch(staged: URL) throws {
        let runningURL = Bundle.main.bundleURL
        try? FileManager.default.trashItem(at: runningURL, resultingItemURL: nil)
        try FileManager.default.moveItem(at: staged, to: runningURL)

        let config = NSWorkspace.OpenConfiguration()
        config.createsNewApplicationInstance = true
        NSWorkspace.shared.openApplication(at: runningURL, configuration: config) { _, _ in
            DispatchQueue.main.async { NSApp.terminate(nil) }
        }
    }

    private static func run(_ launchPath: String, _ arguments: [String]) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: launchPath)
        process.arguments = arguments
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { throw UpdaterError.unzipFailed }
    }

    /// Numeric dot/dash/plus-split comparison, matching `update.rs`'s
    /// `is_newer()`: `v` prefix stripped, components compared numerically.
    static func isNewer(_ candidate: String, than current: String) -> Bool {
        func components(_ s: String) -> [Int] {
            s.trimmingCharacters(in: CharacterSet(charactersIn: "v"))
                .split(whereSeparator: { ".-+".contains($0) })
                .map { Int($0) ?? 0 }
        }
        let a = components(candidate)
        let b = components(current)
        for i in 0..<max(a.count, b.count) {
            let x = i < a.count ? a[i] : 0
            let y = i < b.count ? b[i] : 0
            if x != y { return x > y }
        }
        return false
    }

    enum UpdaterError: Error {
        case downloadFailed
        case checksumMismatch
        case unzipFailed
    }
}
