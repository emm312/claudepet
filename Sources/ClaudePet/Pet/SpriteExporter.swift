import AppKit
import ImageIO
import UniformTypeIdentifiers

/// Rasterizes every frame of every clip in `PetSprites` out to real PNG files in
/// `Resources/sprites/`. Run via `ClaudePet --export-sprites` (see main.swift).
/// This keeps the pixel art authored as compact Swift source (easy to read/diff)
/// while still producing real, inspectable asset files that ship in the bundle
/// and could be swapped for hand-drawn art later without touching the renderer.
enum SpriteExporter {
    enum ExportError: Error {
        case destinationCreationFailed
        case finalizeFailed
    }

    /// Native-resolution (zoom 1) PNGs - the renderer/CALayer does the upscaling
    /// at display time, so assets stay small and crisp at any zoom level.
    static func exportAll(to directory: URL) throws -> [URL] {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)

        var written: [URL] = []
        for (state, clip) in PetSprites.clips.sorted(by: { $0.key.rawValue < $1.key.rawValue }) {
            for (index, frame) in clip.frames.enumerated() {
                let image = PixelArtRenderer.render(grid: frame, zoom: 1)
                let url = directory.appendingPathComponent("\(state.rawValue)_\(index).png")
                try write(image: image, to: url)
                written.append(url)
            }
        }
        return written
    }

    private static func write(image: CGImage, to url: URL) throws {
        guard let destination = CGImageDestinationCreateWithURL(
            url as CFURL, UTType.png.identifier as CFString, 1, nil
        ) else {
            throw ExportError.destinationCreationFailed
        }
        CGImageDestinationAddImage(destination, image, nil)
        guard CGImageDestinationFinalize(destination) else {
            throw ExportError.finalizeFailed
        }
    }
}
