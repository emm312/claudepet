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

    @Test func everySkinCoversEveryAnimStateWithFullSizeFrames() {
        let states = Set(PetSprites.clips.keys)
        for id in SkinId.allCases {
            guard let skin = Skins.all[id] else {
                Issue.record("no SkinDef registered for \(id)")
                continue
            }
            #expect(Set(skin.clips.keys) == states, "\(id) doesn't cover every anim state")
            for (state, clip) in skin.clips {
                for (fi, frame) in clip.frames.enumerated() {
                    #expect(frame.count == Int(PetSprites.gridSize.height), "\(id) \(state) frame \(fi) wrong row count")
                    for row in frame {
                        #expect(row.count == Int(PetSprites.gridSize.width), "\(id) \(state) frame \(fi) wrong row width")
                    }
                }
            }
        }
    }

    @Test func everyAccessoryGridIsFullSize() {
        for id in AccessoryId.allCases {
            guard let accessory = Accessories.all[id] else {
                Issue.record("no AccessoryDef registered for \(id)")
                continue
            }
            #expect(accessory.grid.count == Int(PetSprites.gridSize.height))
            for row in accessory.grid {
                #expect(row.count == Int(PetSprites.gridSize.width))
            }
        }
    }

    @Test func renderCompositeLayersAccessoriesOverTheSkin() {
        guard let skin = Skins.all[.classic], let clip = skin.clips[.idle], let hat = Accessories.all[.topHat] else {
            Issue.record("missing classic idle clip or top hat accessory")
            return
        }
        let plain = PixelArtRenderer.render(grid: clip.frames[0], palette: skin.palette, zoom: 2)
        let withHat = PixelArtRenderer.renderComposite(grid: clip.frames[0], palette: skin.palette, accessories: [hat], zoom: 2)
        #expect(plain.width == withHat.width)
        #expect(plain.height == withHat.height)
    }
}
