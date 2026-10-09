import Foundation
import CFNetwork

/// DNS wire messages are transported only through an explicit loopback proxy.
/// No fallback resolver, redirect, cookie store or persistent response cache.
final class DNSOverProxy: NSObject, URLSessionTaskDelegate {
    private var session: URLSession!
    init(host: String, port: Int) {
        super.init()
        let c = URLSessionConfiguration.ephemeral
        c.connectionProxyDictionary = [
            kCFNetworkProxiesHTTPEnable as String: 1,
            kCFNetworkProxiesHTTPProxy as String: host,
            kCFNetworkProxiesHTTPPort as String: port,
            kCFNetworkProxiesHTTPSEnable as String: 1,
            kCFNetworkProxiesHTTPSProxy as String: host,
            kCFNetworkProxiesHTTPSPort as String: port,
            kCFNetworkProxiesExceptionsList as String: []
        ]
        c.timeoutIntervalForRequest = 8
        c.timeoutIntervalForResource = 10
        c.urlCache = nil
        c.httpCookieStorage = nil
        session = URLSession(configuration: c, delegate: self, delegateQueue: nil)
    }
    func stop() { session.invalidateAndCancel() }
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) { completionHandler(nil) }
    static func validReply(_ bytes: Data, query: Data) -> Bool {
        query.count >= 12 && bytes.count >= 12 && bytes.count <= 65535 && bytes.prefix(2) == query.prefix(2) && bytes[2] & 0x80 != 0
    }
    func query(_ data: Data, completion: @escaping (Data?) -> Void) {
        guard data.count >= 12, data.count <= 65535 else { completion(nil); return }
        // Numeric endpoint avoids resolving the DoH endpoint through this DNS
        // provider itself. TLS certificate validation remains enabled.
        var request = URLRequest(url: URL(string: "https://1.1.1.1/dns-query")!)
        request.httpMethod = "POST"
        request.httpBody = data
        request.setValue("application/dns-message", forHTTPHeaderField: "Content-Type")
        request.setValue("application/dns-message", forHTTPHeaderField: "Accept")
        session.dataTask(with: request) { bytes, response, error in
            guard error == nil, let response = response as? HTTPURLResponse,
                  response.statusCode == 200, response.mimeType == "application/dns-message",
                  let bytes, Self.validReply(bytes, query: data) else { completion(nil); return }
            completion(bytes)
        }.resume()
    }
}
