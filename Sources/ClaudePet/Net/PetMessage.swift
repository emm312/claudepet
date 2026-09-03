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
    /// "express" delivery - the courier rides the horse (windows-branch feature).
    /// Optional-decoded so a message from a build that predates it still loads.
    let express: Bool
    /// `.ack` only: how many more seconds the acker's own visitor animation
    /// (arriving -> handing -> leaving) needs to finish, computed on the
    /// acker's own screen at the moment it received the delivery. Lets the
    /// sender's courier wait exactly that long instead of guessing - see
    /// `Courier.receivedAck`. Optional-decoded so a message from a build that
    /// predates it still loads (the receiver then falls back to
    /// `Courier.defaultWait`).
    let timeToReturn: TimeInterval?
    /// The sender's chosen skin/accessories at send time, so a peer's pet
    /// renders with their own look while visiting rather than always showing
    /// classic. Optional-decoded so a message from a build that predates skins
    /// still loads (the receiver then falls back to `.classic` / no accessories,
    /// same tolerance as `express`).
    let senderSkin: SkinId?
    let senderAccessories: [AccessoryId]?

    private enum CodingKeys: String, CodingKey {
        case id, kind, text, senderName, exitEdge, sentAt, express, timeToReturn, senderSkin, senderAccessories
    }

    init(id: UUID, kind: Kind, text: String, senderName: String, exitEdge: Edge, sentAt: Date, express: Bool = false, timeToReturn: TimeInterval? = nil, senderSkin: SkinId? = nil, senderAccessories: [AccessoryId]? = nil) {
        self.id = id
        self.kind = kind
        self.text = text
        self.senderName = senderName
        self.exitEdge = exitEdge
        self.sentAt = sentAt
        self.express = express
        self.timeToReturn = timeToReturn
        self.senderSkin = senderSkin
        self.senderAccessories = senderAccessories
    }

    // Field-for-field identical to the compiler-synthesized decoder (same keys,
    // same `Date` decoding via the decoder's own strategy - the MultipeerConnectivity
    // path is unchanged), with added tolerances: `express`, `timeToReturn`,
    // `senderSkin`, and `senderAccessories` are optional so a message from a
    // build that predates any of them still loads.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(UUID.self, forKey: .id)
        kind = try c.decode(Kind.self, forKey: .kind)
        text = try c.decode(String.self, forKey: .text)
        senderName = try c.decode(String.self, forKey: .senderName)
        exitEdge = try c.decode(Edge.self, forKey: .exitEdge)
        sentAt = try c.decode(Date.self, forKey: .sentAt)
        express = try c.decodeIfPresent(Bool.self, forKey: .express) ?? false
        timeToReturn = try c.decodeIfPresent(TimeInterval.self, forKey: .timeToReturn)
        senderSkin = try c.decodeIfPresent(SkinId.self, forKey: .senderSkin)
        senderAccessories = try c.decodeIfPresent([AccessoryId].self, forKey: .senderAccessories)
    }

    static func deliver(text: String, senderName: String, exitEdge: Edge, express: Bool = false, senderSkin: SkinId = .classic, senderAccessories: [AccessoryId] = []) -> PetMessage {
        PetMessage(id: UUID(), kind: .deliver, text: text, senderName: senderName, exitEdge: exitEdge, sentAt: Date(), express: express, senderSkin: senderSkin, senderAccessories: senderAccessories)
    }

    /// The ack for a given delivery - correlates by `id` so a stray/duplicate ack
    /// can't resolve the wrong outbound courier. `senderName` is the *acker's*
    /// own name, not the original sender's - cloning the delivery's `senderName`
    /// made the sender upsert its own name into its peer map on receipt instead
    /// of learning the acker's address. `timeToReturn` is how many more seconds
    /// the acker's own visitor animation needs, computed on the acker's screen.
    func makeAck(from localName: String, timeToReturn: TimeInterval) -> PetMessage {
        PetMessage(id: id, kind: .ack, text: "", senderName: localName, exitEdge: exitEdge, sentAt: Date(), express: express, timeToReturn: timeToReturn, senderSkin: senderSkin, senderAccessories: senderAccessories)
    }
}
