import CoreGraphics
import Foundation

/// The express-delivery horse, authored as a pixel grid in the same style as
/// `PetSprites` (flat color blocks, no outline, `.` = transparent) rather than
/// baked from a photo. Faces right: ears and head top-right, a maned neck
/// sloping down into the barrel, rump and tail at the left, four legs below.
/// Two frames give it a gallop cycle: legs gathered under the body, then
/// front legs reaching forward and hind legs driving back.
/// Mirrors `src-win/src/pet/sprites.rs`'s `HORSE_FRAMES`.
enum HorseSprite {
    /// Rows 0-8 are the same in both frames - only the legs (rows 9-11) move.
    private static let torso: [String] = [
        "......................",
        ".................4.4..", // ears
        ".................4444.", // skull
        "................445444", // brow, eye, muzzle
        ".............555444444", // mane over jaw
        "...........554444444..", // mane over neck
        ".554444444444444444...", // tail root + back
        "554444444444444444....", // tail + barrel
        "55.44444444444444.....", // tail + belly
    ]

    /// Gallop's "collected" phase - all four legs gathered under the body.
    private static let frame1: [[UInt8]] = parse(torso + [
        "....44.44....44.44....",
        "....44.44....44.44....",
        "....55.55....55.55....",
    ])

    /// Gallop's "extended" phase - hind legs driving back, fore legs reaching
    /// forward. The upper row stays put so the legs read as swinging from the
    /// body rather than sliding sideways as a whole.
    private static let frame2: [[UInt8]] = parse(torso + [
        "....44.44....44.44....",
        "..44.44........44.44..",
        ".55.55..........55.55.",
    ])

    static let gridSize = CGSize(width: 22, height: 12)

    /// Rendered at the same zoom as the pet's own sprites (`Runtime`'s `zoom`)
    /// so the horse reads at a consistent pixel scale next to it.
    static let frames: [CGImage] = [frame1, frame2].map { PixelArtRenderer.render(grid: $0, zoom: 5) }
    static let frameDuration: TimeInterval = 1.0 / 12.0 // brisk gallop cadence

    /// How far above its normal ground position a rider sits while on the
    /// horse's back - tuned to this sprite's proportions (the back line is row
    /// 6 of 12, above the legs) so the pet reads as sitting on top of the
    /// horse rather than overlapping it at the same height.
    static let riderLift: CGFloat = 28

    private static func parse(_ rows: [String]) -> [[UInt8]] {
        rows.map { row in
            row.map { ch -> UInt8 in
                switch ch {
                case "4": return 4 // hide
                case "5": return 5 // mane, tail, hooves, eye
                default: return 0
                }
            }
        }
    }
}
