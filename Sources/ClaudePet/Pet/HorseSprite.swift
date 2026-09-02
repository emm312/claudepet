import CoreGraphics
import Foundation

/// The express-delivery horse, authored as a pixel grid in the same style as
/// `PetSprites` (flat color blocks, no outline, `.` = transparent) rather than
/// baked from a photo. Two frames give it a simple gallop cycle: legs
/// gathered under the body, then swept front-forward/back-backward.
enum HorseSprite {
    private static let body: [(row: Int, start: Int, end: Int)] = [
        (1, 15, 18), // ear bump
        (2, 13, 21), // head
        (3, 10, 21), // head + neck
        (4, 6, 21),  // neck + body
        (5, 2, 21),  // body main
        (6, 1, 22),  // body widest
    ]
    private static let eye = (row: 3, col: 18)
    private static let tailMark = (row: 6, col: 1)

    /// Frame 1: gallop's "collected" phase - all four legs gathered under the body.
    private static let frame1: [[UInt8]] = makeFrame(legs: [
        (7, 4, 1), (7, 5, 1), (7, 9, 1), (7, 10, 1), (7, 12, 1), (7, 13, 1), (7, 17, 1), (7, 18, 1),
        (8, 4, 2), (8, 5, 2), (8, 9, 2), (8, 10, 2), (8, 12, 2), (8, 13, 2), (8, 17, 2), (8, 18, 2),
    ])

    /// Frame 2: gallop's "extended" phase - front legs swept forward, back legs swept back.
    private static let frame2: [[UInt8]] = makeFrame(legs: [
        (7, 1, 1), (7, 2, 1), (7, 7, 1), (7, 8, 1), (7, 14, 1), (7, 15, 1), (7, 19, 1), (7, 20, 1),
        (8, 1, 2), (8, 2, 2), (8, 7, 2), (8, 8, 2), (8, 14, 2), (8, 15, 2), (8, 19, 2), (8, 20, 2),
    ])

    static let gridSize = CGSize(width: 22, height: 12)

    /// Rendered at the same zoom as the pet's own sprites (`Runtime`'s `zoom`)
    /// so the horse reads at a consistent pixel scale next to it.
    static let frames: [CGImage] = [frame1, frame2].map { PixelArtRenderer.render(grid: $0, zoom: 5) }
    static let frameDuration: TimeInterval = 1.0 / 12.0 // brisk gallop cadence

    /// How far above its normal ground position a rider sits while on the
    /// horse's back - tuned to this sprite's proportions (the body sits
    /// roughly mid-height, above the legs) so the pet reads as sitting on top
    /// of the horse rather than overlapping it at the same height.
    static let riderLift: CGFloat = 28

    /// `legs` entries are `(row, col, paletteIndex)` - 4 (light) for the leg,
    /// 5 (dark) for the hoof, one row below.
    private static func makeFrame(legs: [(Int, Int, Int)]) -> [[UInt8]] {
        var grid = [[UInt8]](repeating: [UInt8](repeating: 0, count: Int(gridSize.width)), count: Int(gridSize.height))
        for span in body {
            for c in span.start..<span.end {
                grid[span.row][c] = 4
            }
        }
        grid[eye.row][eye.col] = 5
        grid[tailMark.row][tailMark.col] = 5
        for (row, col, kind) in legs {
            grid[row][col] = kind == 1 ? 4 : 5
        }
        return grid
    }
}
