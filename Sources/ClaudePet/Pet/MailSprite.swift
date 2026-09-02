import CoreGraphics

/// The mail parcel carried on every courier leg, authored as a pixel grid in
/// the same style as `PetSprites`/`HorseSprite` (flat color blocks, no
/// outline, `.` = transparent) - a plain envelope with a flap and a wax seal.
/// Static (one frame): unlike the horse it doesn't need a gait cycle.
enum MailSprite {
    private static let grid: [[UInt8]] = parse([
        "..................",
        "..................",
        ".5666666666666665.",
        ".6566666666666656.",
        ".6656666666666566.",
        ".6665666336665666.",
        ".6666666666666666.",
        ".6666666666666666.",
        ".6666666666666666.",
        ".6666666666666666.",
        "..................",
        "..................",
    ])

    static let gridSize = CGSize(width: 18, height: 12)

    /// A smaller zoom than the pet's own sprites (`Runtime`'s `zoom`, 5) - at
    /// zoom 5 an 18x12 grid would render almost as big as the pet itself.
    /// This keeps the envelope reading as a small carried object.
    static let image: CGImage = PixelArtRenderer.render(grid: grid, zoom: 2)

    private static func parse(_ rows: [String]) -> [[UInt8]] {
        rows.map { row in
            row.map { ch -> UInt8 in
                switch ch {
                case "3": return 3
                case "5": return 5
                case "6": return 6
                default: return 0
                }
            }
        }
    }
}
