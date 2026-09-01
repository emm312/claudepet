import Foundation

/// Wire format for pet-to-pet messages, JSON-encoded over `PeerTransport`.
///
/// Explicitly `nonisolated`/`Sendable`: this is plain data that crosses from
/// MultipeerConnectivity's background delegate callbacks onto the main actor,
/// so it must not pick up the module's default `MainActor` isolation.
nonisolated struct PetMessage: Codable, Sendable {
    enum Kind: String, Codable, Sendable { case deliver, ack }

    /// Which edge of the sender's screen the pet exited through. The receiver
    /// spawns its visitor on the opposite edge so the trip reads as continuous.
    enum Edge: String, Codable, Sendable {
        case left, right

        var opposite: Edge { self == .left ? .right : .left }
    }

    let id: UUID
    let kind: Kind
    let text: String // empty for .ack
    let senderName: String
    let exitEdge: Edge
    let sentAt: Date

    static func deliver(text: String, senderName: String, exitEdge: Edge) -> PetMessage {
        PetMessage(id: UUID(), kind: .deliver, text: text, senderName: senderName, exitEdge: exitEdge, sentAt: Date())
    }

    /// The ack for a given delivery - correlates by `id` so a stray/duplicate ack
    /// can't resolve the wrong outbound courier.
    func makeAck() -> PetMessage {
        PetMessage(id: id, kind: .ack, text: "", senderName: senderName, exitEdge: exitEdge, sentAt: Date())
    }
}
