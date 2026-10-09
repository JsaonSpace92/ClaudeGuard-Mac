import Foundation

// New implementation using Apple's public APIs. No third-party firewall code.
struct LeakPolicy: Codable {
    let proxyHost: String
    let proxyPort: Int
    let applicationRoots: [String]
    let acknowledgeSystemDNS: Bool

    func validate() throws {
        guard ["127.0.0.1", "::1"].contains(proxyHost), (1...65535).contains(proxyPort),
              !applicationRoots.isEmpty, applicationRoots.count <= 32,
              acknowledgeSystemDNS,
              applicationRoots.allSatisfy({ $0.hasPrefix("/") && $0.hasSuffix(".app") && !$0.contains("/../") })
        else { throw NSError(domain: "ClaudeGuard", code: 1, userInfo: [NSLocalizedDescriptionKey: "代理、应用列表或系统 DNS 范围确认无效"]) }
    }
    func protects(executable: String) -> Bool {
        applicationRoots.contains { executable.hasPrefix($0 + "/Contents/") }
    }
    func allows(host: String, port: Int, proto: Int32) -> Bool {
        // Strict proxy-only mode: includes IPv6 and encrypted DNS. UDP cannot
        // escape through arbitrary STUN/QUIC ports. No broad loopback exception.
        proto == 6 && host == proxyHost && port == proxyPort
    }
}
