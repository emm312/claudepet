import Foundation

/// Discrete mood buckets, derived from stats, that drive sprite choice, movement
/// speed, and which dialogue pool to draw from.
enum Mood: String, Codable {
    case happy, content, hungry, tired, sad, dirty
}

/// Persisted stat block for the pet. All stat fields are 0...100, where 100 is best.
struct PetState: Codable {
    var hunger: Double = 80       // 100 = full, 0 = starving
    var energy: Double = 100      // 100 = well rested, 0 = exhausted
    var happiness: Double = 80    // 100 = delighted, 0 = miserable
    var cleanliness: Double = 100 // 100 = spotless, 0 = filthy

    var birthDate: Date = Date()
    var lastTick: Date = Date()

    /// Install updates automatically in the background. Not pet state, but it
    /// rides along in the same state.json rather than a second config file -
    /// mirrors the Windows port's `PetState.auto_update`. Decoded with a
    /// fallback below so existing state.json files (saved before this field
    /// existed) still decode instead of resetting every stat to default.
    var autoUpdatesEnabled: Bool = true

    /// The pet's chosen look and worn accessories. Rides along in the same
    /// state.json (see `autoUpdatesEnabled` above for the precedent), decoded
    /// with a fallback below so state.json files saved before skins existed
    /// still decode as `.classic` / no accessories rather than failing.
    var skinId: SkinId = .classic
    var accessoryIds: Set<AccessoryId> = []

    private enum CodingKeys: String, CodingKey {
        case hunger, energy, happiness, cleanliness, birthDate, lastTick, autoUpdatesEnabled, skinId, accessoryIds
    }

    init() {}

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        hunger = try container.decode(Double.self, forKey: .hunger)
        energy = try container.decode(Double.self, forKey: .energy)
        happiness = try container.decode(Double.self, forKey: .happiness)
        cleanliness = try container.decode(Double.self, forKey: .cleanliness)
        birthDate = try container.decode(Date.self, forKey: .birthDate)
        lastTick = try container.decode(Date.self, forKey: .lastTick)
        autoUpdatesEnabled = try container.decodeIfPresent(Bool.self, forKey: .autoUpdatesEnabled) ?? true
        skinId = try container.decodeIfPresent(SkinId.self, forKey: .skinId) ?? .classic
        accessoryIds = try container.decodeIfPresent(Set<AccessoryId>.self, forKey: .accessoryIds) ?? []
    }

    /// Per-hour decay rates. Tuned so the pet needs light daily attention but
    /// never spirals from a single missed day.
    private static let hungerDecayPerHour = 3.0
    private static let energyDecayPerHour = 2.0
    private static let happinessDecayPerHour = 1.5
    private static let cleanlinessDecayPerHour = 1.0

    /// Never simulate more than this much elapsed time in one jump, so returning
    /// after a week away doesn't nuke every stat to zero in one tick - the pet
    /// is simply "very neglected", not instantly dead.
    private static let maxCatchUp: TimeInterval = 12 * 3600

    /// Advances stats to `now`, based on elapsed wall-clock time since `lastTick`.
    /// Safe against clock skew: a negative or absurd delta is treated as zero.
    mutating func tick(now: Date = Date()) {
        var elapsed = now.timeIntervalSince(lastTick)
        if !elapsed.isFinite || elapsed < 0 {
            elapsed = 0
        }
        elapsed = min(elapsed, Self.maxCatchUp)
        let hours = elapsed / 3600

        hunger = clamp(hunger - Self.hungerDecayPerHour * hours)
        energy = clamp(energy - Self.energyDecayPerHour * hours)
        happiness = clamp(happiness - Self.happinessDecayPerHour * hours)
        cleanliness = clamp(cleanliness - Self.cleanlinessDecayPerHour * hours)

        lastTick = now
    }

    mutating func feed() {
        hunger = clamp(hunger + 30)
        happiness = clamp(happiness + 5)
    }

    mutating func play() {
        happiness = clamp(happiness + 20)
        energy = clamp(energy - 10)
        hunger = clamp(hunger - 5)
    }

    mutating func clean() {
        cleanliness = 100
    }

    mutating func pet() {
        happiness = clamp(happiness + 8)
    }

    mutating func sleep(hours: Double) {
        energy = clamp(energy + hours * 12)
    }

    private func clamp(_ value: Double) -> Double {
        min(max(value, 0), 100)
    }

    /// Overall mood derived from the worst-off relevant stat, in priority order -
    /// a starving pet reads as hungry even if it's also a bit dirty.
    var mood: Mood {
        if hunger < 25 { return .hungry }
        if energy < 20 { return .tired }
        if cleanliness < 25 { return .dirty }
        if happiness < 30 { return .sad }
        if happiness > 70 && hunger > 60 && energy > 60 { return .happy }
        return .content
    }

    var lifecycleStage: String {
        let days = Date().timeIntervalSince(birthDate) / 86400
        switch days {
        case ..<1: return "egg"
        case 1..<4: return "baby"
        case 4..<10: return "teen"
        default: return "adult"
        }
    }
}

/// Loads/saves `PetState` as JSON in Application Support, atomically and off the
/// main thread's critical path.
enum PetStateStore {
    private static var fileURL: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let dir = base.appendingPathComponent("ClaudePet", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir.appendingPathComponent("state.json")
    }

    static func load() -> PetState {
        guard let data = try? Data(contentsOf: fileURL),
              let state = try? JSONDecoder().decode(PetState.self, from: data) else {
            return PetState()
        }
        return state
    }

    static func save(_ state: PetState) {
        guard let data = try? JSONEncoder().encode(state) else { return }
        try? data.write(to: fileURL, options: .atomic)
    }
}
