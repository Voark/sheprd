import Foundation
import Testing
@testable import SheprdRemoteCore

/// An address that answers and then fails says how, for the computer's log:
/// "The connection failed" alone tells nobody which step broke.
struct LinkLostTests {
    private static func lost(_ updates: [LinkUpdate]) -> [String] {
        updates.compactMap { if case let .lost(line) = $0 { line } else { nil } }
    }

    private static func offline(_ updates: [LinkUpdate]) -> Bool {
        updates.contains { if case .status(.offline) = $0 { true } else { false } }
    }

    @Test func aFailedSignInIsNamedOncePerOutage() async {
        let computer = FakeComputer()
        await computer.setPing("https://lan", mac)
        await computer.failSessions(URLError(.secureConnectionFailed))
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.start()
        let first = await collect(link, until: Self.offline)
        let lines = Self.lost(first)
        #expect(lines.count == 1)
        #expect(lines.first?.hasPrefix("sign-in at https://lan failed: ") == true)
        #expect(lines.first?.contains("NSURLErrorDomain -1200") == true)
        await link.networkChanged()
        let retry = await collect(link, until: Self.offline)
        #expect(Self.lost(retry).isEmpty)
        await link.stop()
    }

    @Test func aFailedSocketIsNamed() async {
        let computer = FakeComputer()
        await computer.setPing("https://lan", mac)
        await computer.failConnects(URLError(.badServerResponse))
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.start()
        let lines = Self.lost(await collect(link, until: Self.offline))
        #expect(lines.first?.hasPrefix("live socket at https://lan failed: ") == true)
        await link.stop()
    }

    @Test func aRefusedDeviceIsRevokedNotLost() async {
        let computer = FakeComputer()
        await computer.setPing("https://lan", mac)
        await computer.failSessions(LinkError.unauthorized)
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.start()
        let seen = await collect(link) { $0.contains(.status(.revoked)) }
        #expect(seen.contains(.status(.revoked)))
        #expect(Self.lost(seen).isEmpty)
        await link.stop()
    }

    @Test func aDroppedSocketSaysHowItEnded() async {
        let computer = FakeComputer()
        await computer.setPing("https://lan", mac)
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.start()
        _ = await collect(link) { $0.contains(.status(.online(origin: "https://lan"))) }
        await computer.drop()
        let lines = Self.lost(await collect(link, until: Self.offline))
        #expect(lines.count == 1)
        #expect(lines.first?.hasPrefix("live socket at https://lan failed: ") == true)
        #expect(lines.first?.contains("NSURLErrorDomain -1005") == true)
        await link.stop()
    }
}
