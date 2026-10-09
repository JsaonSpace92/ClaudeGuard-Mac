import Foundation
import NetworkExtension
import CFNetwork

// OS-delegated DNS cannot reliably be assigned back to the originating app.
// This provider therefore explicitly covers SYSTEM DNS. It only contacts a DoH
// service through the user's existing loopback HTTP/Mixed proxy, with no direct
// fallback. It does not edit Clash, system DNS addresses or proxy preferences.
@objc(DNSProvider)
final class DNSProvider: NEDNSProxyProvider {
    private var transport: DNSOverProxy?
    private let queue = DispatchQueue(label: "local.claudeguard.dns")
    private var flows: [ObjectIdentifier: NEAppProxyFlow] = [:]
    override func startProxy(options: [String: Any]?, completionHandler: @escaping (Error?) -> Void) {
        do {
            guard let data = options?["policy"] as? Data else { throw failure("缺少 DNS 策略") }
            let p = try JSONDecoder().decode(LeakPolicy.self,from:data);try p.validate()
            transport=DNSOverProxy(host:p.proxyHost,port:p.proxyPort)
            completionHandler(nil)
        } catch { completionHandler(error) }
    }
    override func stopProxy(with reason: NEProviderStopReason, completionHandler: @escaping () -> Void) {
        queue.async { self.transport?.stop();self.transport=nil
            for flow in self.flows.values { flow.closeReadWithError(failure("DNS 保护已停止"));flow.closeWriteWithError(failure("DNS 保护已停止")) }
            self.flows.removeAll();completionHandler()
        }
    }
    override func handleNewFlow(_ flow: NEAppProxyFlow) -> Bool {
        queue.async {
            guard self.flows.count < 128 else { self.finish(flow,error:failure("DNS 请求过多"));return }
            self.flows[ObjectIdentifier(flow)]=flow
            flow.open(withLocalEndpoint:nil) { error in self.queue.async {
                if let error { self.finish(flow,error:error);return }
                if let udp=flow as? NEAppProxyUDPFlow { self.readUDP(udp) }
                else if let tcp=flow as? NEAppProxyTCPFlow { self.readTCP(tcp,buffer:Data()) }
                else { self.finish(flow,error:failure("不支持的 DNS 流")) }
            }}
        };return true
    }
    private func finish(_ flow: NEAppProxyFlow,error:Error?) {
        flow.closeReadWithError(error);flow.closeWriteWithError(error);flows.removeValue(forKey:ObjectIdentifier(flow))
    }
    private func query(_ data:Data,completion:@escaping (Data?)->Void) {
        guard let transport else {completion(nil);return}
        transport.query(data,completion:completion)
    }
    private func readUDP(_ flow:NEAppProxyUDPFlow) {
        flow.readDatagrams { packets,endpoints,error in self.queue.async {
            guard error==nil,let packets,let endpoints,!packets.isEmpty,packets.count==endpoints.count,packets.count<=32 else {self.finish(flow,error:error);return}
            self.replyUDP(flow,packets:packets,endpoints:endpoints,index:0)
        }}
    }
    private func replyUDP(_ flow:NEAppProxyUDPFlow,packets:[Data],endpoints:[NWEndpoint],index:Int) {
        guard index<packets.count else {readUDP(flow);return}
        query(packets[index]) { response in self.queue.async {
            guard let response else {self.finish(flow,error:failure("代理 DNS 不可用，未回退直连"));return}
            flow.writeDatagrams([response],sentBy:[endpoints[index]]) { error in self.queue.async {
                if let error {self.finish(flow,error:error)} else {self.replyUDP(flow,packets:packets,endpoints:endpoints,index:index+1)}
            }}
        }}
    }
    private func readTCP(_ flow:NEAppProxyTCPFlow,buffer:Data) {
        if buffer.count>=2 {
            let n=Int(buffer[0])*256+Int(buffer[1])
            guard n>=12 else {finish(flow,error:failure("无效 DNS 长度"));return}
            if buffer.count>=n+2 {
                let rest=Data(buffer.dropFirst(n+2))
                query(Data(buffer.dropFirst(2).prefix(n))) { reply in self.queue.async {
                    guard let reply else {self.finish(flow,error:failure("代理 DNS 不可用，未回退直连"));return}
                    var framed=Data([UInt8(reply.count>>8),UInt8(reply.count&255)]);framed.append(reply)
                    flow.write(framed) { error in self.queue.async {
                        if let error {self.finish(flow,error:error)} else {self.readTCP(flow,buffer:rest)}
                    }}
                }};return
            }
        }
        guard buffer.count<=131072 else {finish(flow,error:failure("DNS 缓冲区超限"));return}
        flow.readData { data,error in self.queue.async {
            guard error==nil,let data,!data.isEmpty else {self.finish(flow,error:error);return}
            var next=buffer;next.append(data);self.readTCP(flow,buffer:next)
        }}
    }
}
