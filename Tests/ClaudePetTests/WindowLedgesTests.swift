import Testing
@testable import ClaudePet

struct WindowLedgesTests {

    @Test func ledgeBelowPicksHighestQualifyingLedge() {
        let ledges = [
            Ledge(minX: 0, maxX: 500, y: 0),     // screen floor
            Ledge(minX: 100, maxX: 300, y: 200), // a window's top edge
            Ledge(minX: 100, maxX: 300, y: 400), // a taller window overlapping in x
        ]
        let result = WindowLedges.ledgeBelow(x: 150, y: 250, in: ledges)
        #expect(result?.y == 200)
    }

    @Test func ledgeBelowIgnoresLedgesNotSpanningX() {
        let ledges = [
            Ledge(minX: 0, maxX: 500, y: 0),
            Ledge(minX: 600, maxX: 800, y: 300), // out of x range
        ]
        let result = WindowLedges.ledgeBelow(x: 150, y: 250, in: ledges)
        #expect(result?.y == 0)
    }

    @Test func ledgeBelowReturnsNilWhenNothingQualifies() {
        let ledges = [Ledge(minX: 0, maxX: 500, y: 300)] // only above the drop point
        let result = WindowLedges.ledgeBelow(x: 150, y: 250, in: ledges)
        #expect(result == nil)
    }
}
