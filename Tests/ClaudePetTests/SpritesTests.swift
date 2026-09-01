import Testing
@testable import ClaudePet
import CoreFoundation

struct SpritesTests {

    @Test func pixelArtRendererProducesCorrectDimensions() {
        let grid: [[UInt8]] = [
            [0, 1],
            [1, 0],
        ]
        let image = PixelArtRenderer.render(grid: grid, zoom: 4)
        #expect(image.width == 8)
        #expect(image.height == 8)
    }

    @Test func transparentIndexProducesZeroAlpha() {
        let grid: [[UInt8]] = [[0, 1]]
        let image = PixelArtRenderer.render(grid: grid, zoom: 2)
        guard let data = image.dataProvider?.data, let ptr = CFDataGetBytePtr(data) else {
            Issue.record("no pixel data")
            return
        }
        // First pixel (0,0) came from index 0 -> transparent -> alpha byte 0.
        let bytesPerPixel = image.bitsPerPixel / 8
        #expect(ptr[bytesPerPixel - 1] == 0)
    }

    @Test func allClipsHaveAtLeastOneFrame() {
        for (state, clip) in PetSprites.clips {
            #expect(!clip.frames.isEmpty, "\(state) has no frames")
        }
    }
}
