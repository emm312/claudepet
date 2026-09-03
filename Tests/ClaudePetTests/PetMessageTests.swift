import Testing
@testable import ClaudePet
import Foundation

struct PetMessageTests {

    @Test func senderSkinAndAccessoriesRoundTripThroughEncodeDecode() throws {
        let message = PetMessage.deliver(
            text: "hi",
            senderName: "DeskA",
            exitEdge: .right,
            senderSkin: .plant,
            senderAccessories: [.topHat, .glasses]
        )
        let data = try JSONEncoder().encode(message)
        let decoded = try JSONDecoder().decode(PetMessage.self, from: data)
        #expect(decoded.senderSkin == .plant)
        #expect(decoded.senderAccessories == [.topHat, .glasses])
    }

    @Test func messageJSONMissingSkinFieldsDecodesWithNilSkin() throws {
        // Shaped like a message from a build that predates skins.
        let json = """
        {"id":"7F3A1C2B-4D5E-4A7B-8C9D-0E1F2A3B4C5D","kind":"deliver","text":"hi","senderName":"DeskA","exitEdge":"right","sentAt":0,"express":false}
        """
        let decoded = try JSONDecoder().decode(PetMessage.self, from: Data(json.utf8))
        #expect(decoded.senderSkin == nil)
        #expect(decoded.senderAccessories == nil)
    }

    @Test func makeAckCarriesForwardTheOriginalSendersSkin() {
        let message = PetMessage.deliver(text: "hi", senderName: "DeskA", exitEdge: .left, senderSkin: .clown, senderAccessories: [.glasses])
        let ack = message.makeAck(from: "DeskB", timeToReturn: 1.5)
        #expect(ack.senderSkin == .clown)
        #expect(ack.senderAccessories == [.glasses])
    }
}
