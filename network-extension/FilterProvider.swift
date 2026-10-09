import Foundation
import NetworkExtension
import Security

@objc(FilterProvider)
final class FilterProvider: NEFilterDataProvider {
    private var policy: LeakPolicy?
    override func startFilter(completionHandler: @escaping (Error?) -> Void) {
        do {
            guard let data = filterConfiguration.vendorConfiguration?["policy"] as? Data else { throw failure("缺少保护策略") }
            let p = try JSONDecoder().decode(LeakPolicy.self, from: data); try p.validate(); policy = p
            let rule = NENetworkRule(remoteNetwork: nil, remotePrefix: 0, localNetwork: nil, localPrefix: 0, protocol: .any, direction: .outbound)
            apply(NEFilterSettings(rules: [NEFilterRule(networkRule: rule, action: .filterData)], defaultAction: .allow), completionHandler: completionHandler)
        } catch { completionHandler(error) }
    }
    override func handleNewFlow(_ flow: NEFilterFlow) -> NEFilterNewFlowVerdict {
        guard let p = policy else { return .drop() }
        // The responsible application and actual socket process can differ
        // for system-managed flows. Match either live audit identity, never a PID.
        var tokens = [flow.sourceAppAuditToken].compactMap { $0 }
        if #available(macOS 13.0, *), let token = flow.sourceProcessAuditToken,
           !tokens.contains(token) { tokens.append(token) }
        let paths = tokens.compactMap { executablePath($0) }
        guard !paths.isEmpty else {
            // System and unidentified flows cannot safely be classified as a
            // selected app. This is reported as a coverage limit, not safety.
            return .allow()
        }
        guard paths.contains(where: { p.protects(executable: $0) }) else { return .allow() }
        guard let socket = flow as? NEFilterSocketFlow,
              let endpoint = socket.remoteEndpoint as? NWHostEndpoint,
              let port = Int(endpoint.port) else { return .drop() }
        return p.allows(host: endpoint.hostname, port: port, proto: socket.socketProtocol) ? .allow() : .drop()
    }
    private func executablePath(_ token: Data) -> String? {
        var code: SecCode?
        let attrs = [kSecGuestAttributeAudit as String: token] as CFDictionary
        guard SecCodeCopyGuestWithAttributes(nil, attrs, [], &code) == errSecSuccess, let code else { return nil }
        var staticCode: SecStaticCode?
        guard SecCodeCopyStaticCode(code, [], &staticCode) == errSecSuccess, let staticCode else { return nil }
        var info: CFDictionary?
        guard SecCodeCopySigningInformation(staticCode, [], &info) == errSecSuccess,
              let dictionary = info as? [String: Any], let url = dictionary[kSecCodeInfoMainExecutable as String] as? URL else { return nil }
        return url.resolvingSymlinksInPath().path
    }
}
func failure(_ text: String) -> NSError { NSError(domain:"ClaudeGuard.Network", code:1,userInfo:[NSLocalizedDescriptionKey:text]) }
