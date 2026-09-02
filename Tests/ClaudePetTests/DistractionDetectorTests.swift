import Testing
@testable import ClaudePet
import Foundation

struct DistractionDetectorTests {

    // MARK: - Positive matches: the reels feed itself

    @Test func acceptsBareReelsPath() {
        #expect(DistractionDetector.urlIsReels(URL(string: "https://instagram.com/reels")!))
    }

    @Test func acceptsWwwSubdomainWithTrailingSlash() {
        #expect(DistractionDetector.urlIsReels(URL(string: "https://www.instagram.com/reels/")!))
    }

    @Test func acceptsReelsPathWithQueryString() {
        #expect(DistractionDetector.urlIsReels(URL(string: "https://www.instagram.com/reels/?foo=1")!))
    }

    // MARK: - Negative: Instagram, but not the reels feed

    @Test func rejectsInstagramHomeFeed() {
        #expect(!DistractionDetector.urlIsReels(URL(string: "https://instagram.com/")!))
    }

    @Test func rejectsInstagramDirectMessages() {
        #expect(!DistractionDetector.urlIsReels(URL(string: "https://instagram.com/direct/inbox")!))
    }

    @Test func rejectsASingleSharedReelNotTheFeed() {
        // Singular "/reel/<id>" - a link to one reel, not the endless feed.
        #expect(!DistractionDetector.urlIsReels(URL(string: "https://instagram.com/reel/abc123")!))
    }

    // MARK: - Negative: host spoofing

    @Test func rejectsLookalikeHost() {
        #expect(!DistractionDetector.urlIsReels(URL(string: "https://notinstagram.com/reels")!))
    }

    @Test func rejectsInstagramAsASubdomainOfAnotherDomain() {
        #expect(!DistractionDetector.urlIsReels(URL(string: "https://instagram.com.evil.co/reels")!))
    }

    @Test func rejectsUnrelatedHostWithReelsPath() {
        #expect(!DistractionDetector.urlIsReels(URL(string: "https://news.example.com/reels")!))
    }
}
