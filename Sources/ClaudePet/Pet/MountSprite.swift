import AppKit

/// What the courier rides on an express delivery. Persisted on
/// `PetState.mountId` and carried on outbound `PetMessage`s (`senderMount`) so
/// the receiving screen's visitor arrives on the sender's own mount, at that
/// mount's speed. Mirrors `MountId` in `src-win/src/pet/sprites.rs`.
enum MountId: String, Codable, CaseIterable {
    case brownHorse, whiteHorse, blackHorse, motorbike

    var displayName: String {
        switch self {
        case .brownHorse: return "Brown Horse"
        case .whiteHorse: return "White Horse"
        case .blackHorse: return "Black Horse"
        case .motorbike: return "Motorbike"
        }
    }

    /// Courier speed multiplier while riding. Keep in sync with `MountDef::
    /// speed_mult` in `src-win/src/pet/sprites.rs`. The horses all match
    /// `Courier.expressSpeedMultiplier`.
    var speedMultiplier: CGFloat {
        switch self {
        case .brownHorse, .whiteHorse, .blackHorse: return Courier.expressSpeedMultiplier
        case .motorbike: return 4.0
        }
    }
}

/// Per-mount sprites: the horse recolors reuse `HorseSprite.grids` with
/// palette indices 4/5 swapped; the motorbike is its own (wider, 28-col)
/// grid. Every mount is 12 rows tall with its riding surface around rows
/// 4-6, so the one shared `riderLift` keeps hooves/wheels on the ground and
/// the rider seated.
/// Mirrors `MOUNTS` in `src-win/src/pet/sprites.rs`.
enum MountSprite {
    static let riderLift: CGFloat = HorseSprite.riderLift

    static func grids(for mount: MountId) -> [[[UInt8]]] {
        mount == .motorbike ? motorbikeGrids : HorseSprite.grids
    }

    static func palette(for mount: MountId) -> [UInt8: NSColor] {
        func horse(body: NSColor, mane: NSColor) -> [UInt8: NSColor] {
            var p = Palette.colors
            p[4] = body
            p[5] = mane
            return p
        }
        switch mount {
        case .brownHorse:
            return Palette.colors
        case .whiteHorse:
            return horse(
                body: NSColor(calibratedRed: 0.91, green: 0.894, blue: 0.863, alpha: 1),
                mane: NSColor(calibratedRed: 0.588, green: 0.573, blue: 0.549, alpha: 1)
            )
        case .blackHorse:
            // The mane is lighter than the body so it still reads against it.
            return horse(
                body: NSColor(calibratedRed: 0.157, green: 0.141, blue: 0.133, alpha: 1),
                mane: NSColor(calibratedRed: 0.431, green: 0.408, blue: 0.384, alpha: 1)
            )
        case .motorbike:
            return [
                1: NSColor(calibratedRed: 0.11, green: 0.11, blue: 0.118, alpha: 1),  // tyres
                2: NSColor(calibratedRed: 0.784, green: 0.188, blue: 0.173, alpha: 1), // red body + tank
                3: NSColor(calibratedRed: 0.698, green: 0.714, blue: 0.737, alpha: 1), // chrome
                4: NSColor(calibratedRed: 0.98, green: 0.839, blue: 0.361, alpha: 1),  // headlight
                5: NSColor(calibratedRed: 0.188, green: 0.157, blue: 0.149, alpha: 1), // seat
            ]
        }
    }

    static func frameDuration(for mount: MountId) -> TimeInterval {
        mount == .motorbike ? 1.0 / 16.0 : HorseSprite.frameDuration
    }

    /// Rendered at the pet's own zoom, like `HorseSprite.frames`, once per
    /// mount up front.
    static func frames(for mount: MountId) -> [CGImage] {
        allFrames[mount] ?? HorseSprite.frames
    }

    private static let allFrames: [MountId: [CGImage]] = Dictionary(uniqueKeysWithValues: MountId.allCases.map { mount in
        let palette = palette(for: mount)
        return (mount, grids(for: mount).map { PixelArtRenderer.render(grid: $0, palette: palette, zoom: 5) })
    })

    /// A red motorbike facing right like the horse: handlebar, fork and
    /// headlight at the front, a seat behind the tank, a tail fender, and a
    /// chrome engine and exhaust slung low between two wheels. 28 columns wide
    /// (the horse is 22) so those parts clear the rider, who covers the middle.
    /// The two frames differ only in the spokes (`+` then `x`), so the wheels
    /// read as spinning. Mirrors `motorbike_mount` in `src-win/src/pet/sprites.rs`.
    private static let motorbikeTop: [String] = [
        "............................",
        "............................",
        ".....................13333..", // handlebar + grip
        "........................3...", // fork
        "........555555.222222...344.", // seat, tank, headlight
        ".2222222222222222222222.34..", // tail fender, body
        "222.....22222222222222..3...", // rear fender, lower body
    ]

    private static let motorbikeGrids: [[[UInt8]]] = [
        parse(motorbikeTop + [
            "..111.....3333333......111..",
            ".1.3.1...333333333....1.3.1.", // engine, exhaust
            ".133313333333333......13331.",
            ".1.3.1.........33.....1.3.1.",
            "..111..................111..",
        ]),
        parse(motorbikeTop + [
            "..111.....3333333......111..",
            ".13.31...333333333....13.31.",
            ".1.3.13333333333......1.3.1.",
            ".13.31.........33.....13.31.",
            "..111..................111..",
        ]),
    ]

    private static func parse(_ rows: [String]) -> [[UInt8]] {
        rows.map { row in row.map { UInt8(String($0)) ?? 0 } }
    }
}
