import AppKit

/// A single named animation clip: a sequence of pixel-grid frames plus how long
/// each frame stays on screen.
struct SpriteClip {
    let frames: [[[UInt8]]] // frame -> row -> palette index (0 = transparent)
    let frameDuration: TimeInterval
    let loops: Bool
}

/// Palette index -> RGBA color. Index 0 is always transparent. Flat two-tone
/// palette matching the Claude Code mascot: terracotta body, no outline,
/// square black eyes.
enum Palette {
    static let colors: [UInt8: NSColor] = [
        1: NSColor(calibratedRed: 0.776, green: 0.455, blue: 0.345, alpha: 1), // body
        2: NSColor(calibratedRed: 0.078, green: 0.078, blue: 0.078, alpha: 1), // eyes
        3: NSColor(calibratedRed: 0.808, green: 0.282, blue: 0.235, alpha: 1), // angry body tint
        4: NSColor(calibratedRed: 0.541, green: 0.361, blue: 0.220, alpha: 1), // horse body (Pet/HorseSprite.swift)
        5: NSColor(calibratedRed: 0.216, green: 0.137, blue: 0.086, alpha: 1), // horse mane/tail/hooves; mail flap line
        6: NSColor(calibratedRed: 0.965, green: 0.941, blue: 0.898, alpha: 1), // mail envelope (Pet/MailSprite.swift)
    ]
}

/// Rasterizes a pixel grid (rows of palette indices) into a CGImage, nearest-
/// neighbour ready. `zoom` repeats each source pixel into a `zoom x zoom` block
/// so we get crisp square pixels at any window scale.
enum PixelArtRenderer {
    static func render(grid: [[UInt8]], palette: [UInt8: NSColor] = Palette.colors, zoom: Int) -> CGImage {
        let rows = grid.count
        let cols = grid.first?.count ?? 0
        let width = cols * zoom
        let height = rows * zoom

        var pixels = [UInt8](repeating: 0, count: width * height * 4)

        for (r, row) in grid.enumerated() {
            for (c, index) in row.enumerated() {
                let color = palette[index]?.usingColorSpace(.deviceRGB)
                let (red, green, blue, alpha): (UInt8, UInt8, UInt8, UInt8)
                if let color {
                    red = UInt8(color.redComponent * 255)
                    green = UInt8(color.greenComponent * 255)
                    blue = UInt8(color.blueComponent * 255)
                    alpha = 255
                } else {
                    (red, green, blue, alpha) = (0, 0, 0, 0)
                }

                for zy in 0..<zoom {
                    for zx in 0..<zoom {
                        let px = c * zoom + zx
                        let py = r * zoom + zy
                        let offset = (py * width + px) * 4
                        pixels[offset + 0] = red
                        pixels[offset + 1] = green
                        pixels[offset + 2] = blue
                        pixels[offset + 3] = alpha
                    }
                }
            }
        }

        let colorSpace = CGColorSpaceCreateDeviceRGB()
        let bitmapInfo = CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue)
        let context = CGContext(
            data: &pixels,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: width * 4,
            space: colorSpace,
            bitmapInfo: bitmapInfo.rawValue
        )!
        return context.makeImage()!
    }

    /// Renders `grid` under `palette`, then draws each `accessory` grid/palette
    /// pair on top in order - used to layer worn accessories (their own small,
    /// mostly-transparent grid with their own tiny palette) over a skin's
    /// frame without merging unrelated palette-index spaces.
    static func renderComposite(grid: [[UInt8]], palette: [UInt8: NSColor], accessories: [AccessoryDef], zoom: Int) -> CGImage {
        var image = render(grid: grid, palette: palette, zoom: zoom)
        guard !accessories.isEmpty else { return image }

        let width = image.width
        let height = image.height
        let colorSpace = CGColorSpaceCreateDeviceRGB()
        let bitmapInfo = CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue)
        guard let context = CGContext(
            data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
            space: colorSpace, bitmapInfo: bitmapInfo.rawValue
        ) else { return image }

        for accessory in accessories {
            let overlay = render(grid: accessory.grid, palette: accessory.palette, zoom: zoom)
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            context.draw(overlay, in: CGRect(x: 0, y: 0, width: width, height: height))
            image = context.makeImage() ?? image
        }
        return image
    }
}

/// The pet's pixel-grid sprites, authored directly in Swift. `.` = transparent.
/// A blocky, flat-color critter in the same style as the Claude Code mascot:
/// a rounded-square head/body with corner notches, stub arms sticking out to
/// the sides, square eyes, and comb-like legs. Grids generated procedurally
/// (rectangle placement, not hand-typed) so proportions stay consistent across
/// frames - see `SpriteExporter` to regenerate PNG assets after an art change.
enum PetSprites {
    private static let idle1: [[UInt8]] = parse([
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "....11111111....",
        "...1111111111...",
        "...1122112211...",
        "1111122112211111",
        "1111111111111111",
        "...1111111111...",
        "...1111111111...",
        "...1111..1111...",
        "...1111..1111...",
    ])

    private static let idle2: [[UInt8]] = parse([
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "....11111111....",
        "...1111111111...",
        "...1111111111...",
        "1111122112211111",
        "1111111111111111",
        "...1111111111...",
        "...1111111111...",
        "...1111..1111...",
        "...1111..1111...",
    ])

    private static let walk1: [[UInt8]] = parse([
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "....11111111....",
        "...1111111111...",
        "...1122112211...",
        "1111122112211111",
        "1111111111111111",
        "...1111111111...",
        "...1111111111...",
        "...1111..1111...",
        "...1111.........",
    ])

    private static let walk2: [[UInt8]] = parse([
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "....11111111....",
        "...1111111111...",
        "...1122112211...",
        "1111122112211111",
        "1111111111111111",
        "...1111111111...",
        "...1111111111...",
        "...1111..1111...",
        ".........1111...",
    ])

    private static let sleep1: [[UInt8]] = parse([
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "....11111111....",
        "...1111111111...",
        "...1111111111...",
        "1111122112211111",
        "1111111111111111",
        "...1111111111...",
        "...1111111111...",
        ".....111111.....",
        "................",
    ])

    private static let sad1: [[UInt8]] = parse([
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "....11111111....",
        "...1111111111...",
        "...1111111111...",
        "1111122112211111",
        "1111122112211111",
        "...1111111111...",
        "...1111111111...",
        "...1111..1111...",
        "...1111..1111...",
    ])

    private static let angry1: [[UInt8]] = parse([
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "....33333333....",
        "...3333333333...",
        "3333333333333...",
        "3333322332233333",
        "...3322332233333",
        "...3333333333...",
        "...3333333333...",
        "...3333.333.....",
        "...3333.........",
    ])

    private static let angry2: [[UInt8]] = parse([
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "....33333333....",
        "...3333333333...",
        "...3333333333...",
        "3333322332233333",
        "3333322332233333",
        "...3333333333...",
        "...3333333333...",
        ".....333.3333...",
        ".........3333...",
    ])

    private static let fall1: [[UInt8]] = parse([
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "....11111111....",
        "1111111111111111",
        "1111221111221111",
        "...1221111221...",
        "...1111111111...",
        "...1111111111...",
        "...1111111111...",
        "...1111.111.....",
        "...1111.........",
    ])

    private static let fall2: [[UInt8]] = parse([
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "....11111111....",
        "1111111111111111",
        "1111221111221111",
        "...1221111221...",
        "...1111111111...",
        "...1111111111...",
        "...1111111111...",
        ".....111.1111...",
        ".........1111...",
    ])

    // Play-triggered hop: alternates a crouch (arms out, legs wide) with a
    // jump (arms raised above the head, legs together) for a bouncy hop.
    private static let jumpCrouch: [[UInt8]] = parse([
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "....11111111....",
        "...1111111111...",
        "...1111111111...",
        "...1122112211...",
        "1111122112211111",
        "1111111111111111",
        "..1111....1111..",
        "..1111....1111..",
        "................",
    ])

    private static let jumpAirborne: [[UInt8]] = parse([
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "..11......11....",
        "....11111111....",
        "...1111111111...",
        "...1122112211...",
        "1111122112211111",
        "1111111111111111",
        "...1111111111...",
        "...1111111111...",
        "....111111......",
        "....111111......",
    ])

    /// Shifts every row of a grid horizontally by `amount` columns (positive =
    /// right), dropping anything pushed past the edge rather than wrapping.
    /// Used to turn the crouch/airborne poses into full lean-left/lean-right
    /// hops without hand-authoring yet more frames.
    private static func shiftColumns(_ grid: [[UInt8]], by amount: Int) -> [[UInt8]] {
        grid.map { row in
            var shifted = [UInt8](repeating: 0, count: row.count)
            for (i, value) in row.enumerated() {
                let j = i + amount
                if j >= 0 && j < row.count {
                    shifted[j] = value
                }
            }
            return shifted
        }
    }

    private static let jumpLeanLeft = shiftColumns(jumpCrouch, by: -2)
    private static let jumpLeanRight = shiftColumns(jumpCrouch, by: 2)
    private static let jumpAirborneLeft = shiftColumns(jumpAirborne, by: -1)
    private static let jumpAirborneRight = shiftColumns(jumpAirborne, by: 1)

    // Feed-triggered eating: an open-mouth chew frame (a black patch below the
    // eyes) alternated with the plain idle pose for a closed-mouth chew.
    private static let eatOpen: [[UInt8]] = parse([
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "....11111111....",
        "...1111111111...",
        "...1122112211...",
        "1111122112211111",
        "1111112222111111",
        "...1111111111...",
        "...1111111111...",
        "...1111..1111...",
        "...1111..1111...",
    ])

    private static let dragged1: [[UInt8]] = parse([
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "....11111111....",
        "...1111111111...",
        "1111122112211...",
        "1111122112211111",
        "...1111111111111",
        "...1111111111...",
        "...1111111111...",
        ".....111111.....",
        "................",
    ])

    private static func parse(_ rows: [String]) -> [[UInt8]] {
        rows.map { row in
            row.map { ch -> UInt8 in
                switch ch {
                case "1": return 1
                case "2": return 2
                case "3": return 3
                default: return 0
                }
            }
        }
    }

    static let clips: [PetMood.AnimState: SpriteClip] = [
        .idle: SpriteClip(frames: [idle1, idle1, idle1, idle2], frameDuration: 0.5, loops: true),
        .walk: SpriteClip(frames: [walk1, walk2], frameDuration: 1.0 / 6.0, loops: true),
        .sleep: SpriteClip(frames: [sleep1], frameDuration: 1.0, loops: true),
        .sad: SpriteClip(frames: [sad1, idle1], frameDuration: 0.8, loops: true),
        .dragged: SpriteClip(frames: [dragged1], frameDuration: 0.2, loops: true),
        .angry: SpriteClip(frames: [angry1, angry2], frameDuration: 1.0 / 10.0, loops: true),
        .fall: SpriteClip(frames: [fall1, fall2], frameDuration: 1.0 / 8.0, loops: true),
        .eat: SpriteClip(frames: [eatOpen, idle1], frameDuration: 0.22, loops: true),
        .jump: SpriteClip(
            frames: [jumpLeanLeft, jumpAirborneLeft, jumpCrouch, jumpAirborne, jumpLeanRight, jumpAirborneRight, jumpCrouch, jumpAirborne],
            frameDuration: 0.11,
            loops: true
        ),
    ]

    /// Native grid size in "pixels" (before zoom).
    static let gridSize = CGSize(width: 16, height: 16)
}
