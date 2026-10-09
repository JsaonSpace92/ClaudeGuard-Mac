import Foundation
@main
struct Tests {
    static func main() throws {
        let p = LeakPolicy(proxyHost:"127.0.0.1",proxyPort:7897,applicationRoots:["/Applications/Fixture.app"],acknowledgeSystemDNS:true)
        try p.validate()
        assert(p.protects(executable:"/Applications/Fixture.app/Contents/MacOS/Fixture"))
        assert(p.protects(executable:"/Applications/Fixture.app/Contents/Frameworks/Helper.app/Contents/MacOS/Helper"))
        assert(!p.protects(executable:"/Applications/Fixture.app.evil/Contents/MacOS/Fixture"))
        assert(!p.protects(executable:"/Applications/Other.app/Contents/MacOS/Other"))
        assert(p.allows(host:"127.0.0.1",port:7897,proto:6))
        for proto:Int32 in [6,17] {for host in ["203.0.113.1","2001:db8::5"] {for port in [53,443,3478,19302,65535] {assert(!p.allows(host:host,port:port,proto:proto))}}}
        assert(!p.allows(host:"127.0.0.1",port:7897,proto:17))
        assert(!p.allows(host:"127.0.0.1",port:53,proto:6))
        let invalid = LeakPolicy(proxyHost:"203.0.113.1",proxyPort:7897,applicationRoots:p.applicationRoots,acknowledgeSystemDNS:true)
        do {try invalid.validate();fatalError("Accepted non-loopback proxy")} catch {}
        var query=Data(repeating:0,count:12);query[0]=12;query[1]=34
        var reply=query;reply[2]=0x80
        assert(DNSOverProxy.validReply(reply,query:query))
        reply[1]=35;assert(!DNSOverProxy.validReply(reply,query:query))
        assert(!DNSOverProxy.validReply(query,query:query))
        assert(!DNSOverProxy.validReply(Data([0]),query:query))
        print("Policy tests passed: exact app boundaries, helper coverage, UDP/TCP/IPv6 denial, exact proxy endpoint, DNS response validation.")
        // Optional local-only integration test: a mock HTTP proxy refuses CONNECT.
        // The proxy test runner checks the received destination; no real DoH traffic.
        if let port=ProcessInfo.processInfo.environment["CG_TEST_PROXY_PORT"].flatMap(Int.init) {
            let dns=DNSOverProxy(host:"127.0.0.1",port:port)
            let semaphore=DispatchSemaphore(value:0)
            dns.query(query) { result in assert(result==nil,"A refused proxy must not produce a DNS reply");semaphore.signal() }
            assert(semaphore.wait(timeout:.now()+15) == .success)
            dns.stop();print("DNS proxy refusal returned failure without a successful direct fallback.")
        }
    }
}
