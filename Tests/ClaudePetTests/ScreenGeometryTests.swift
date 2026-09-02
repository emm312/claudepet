import Testing
@testable import ClaudePet
import AppKit

struct ScreenGeometryTests {

    @Test func flipsTopLeftOriginToBottomLeftOrigin() throws {
        let primaryHeight = try #require(NSScreen.screens.first?.frame.height)

        let topLeft = CGRect(x: 50, y: 20, width: 200, height: 100)
        let result = try #require(ScreenGeometry.appKitRect(fromTopLeft: topLeft))

        // x/width/height pass through unchanged; only y (the top edge in
        // top-left coords) flips to the bottom edge in bottom-left coords.
        #expect(result.origin.x == topLeft.origin.x)
        #expect(result.width == topLeft.width)
        #expect(result.height == topLeft.height)
        #expect(result.origin.y == primaryHeight - topLeft.origin.y - topLeft.height)
    }

    @Test func roundTripsBackToTheOriginalTopEdge() throws {
        let primaryHeight = try #require(NSScreen.screens.first?.frame.height)

        let topLeft = CGRect(x: 0, y: 40, width: 300, height: 150)
        let flipped = try #require(ScreenGeometry.appKitRect(fromTopLeft: topLeft))

        // The AppKit rect's top edge (maxY) should land back at the original
        // top-left rect's top edge, measured from the bottom of the screen.
        #expect(flipped.maxY == primaryHeight - topLeft.minY)
    }
}
