import Foundation
import NetworkExtension
import SystemExtensions
import CoreLocation

// Embedded command helper. Never run during app startup or automatically arm.
@main
final class Controller: NSObject, OSSystemExtensionRequestDelegate {
    private var policyData=Data()
    private var extensionID: String { Bundle.main.object(forInfoDictionaryKey:"CGNetworkExtensionIdentifier") as? String ?? "local.claudeguard.mac.network" }
    private func emit(_ object:[String:Any],code:Int32=0) -> Never {
        let data=(try? JSONSerialization.data(withJSONObject:object,options:[.sortedKeys])) ?? Data("{}".utf8)
        FileHandle.standardOutput.write(data);FileHandle.standardOutput.write(Data([10]));exit(code)
    }
    private func fail(_ error:Error) -> Never { emit(["state":"error","message":error.localizedDescription],code:1) }
    static func main() {
        let controller=Controller()
        DispatchQueue.main.asyncAfter(deadline:.now()+50) { controller.emit(["state":"unknown","message":"系统操作尚未完成，请检查系统扩展批准状态后刷新"],code:1) }
        controller.run();RunLoop.main.run()
    }
    private func run() {
        let action=CommandLine.arguments.dropFirst().first ?? "status"
        if action=="environment" {
            // Read only: never request location permission or coordinates.
            emit(["state":"observed","timezone":TimeZone.current.identifier,
                  "languages":Locale.preferredLanguages,"locale":Locale.current.identifier,
                  "locationServicesEnabled":CLLocationManager.locationServicesEnabled()])
        }
        // Unsigned/ad-hoc builds must never request or install network settings.
        guard Bundle.main.object(forInfoDictionaryKey:"CGNetworkProvisioned") as? Bool == true else {
            emit(["state":"unavailable","message":"当前为普通签名构建，网络阻止尚不可用；需要 Developer ID 与 Network Extension 配置。检测功能可使用。"])
        }
        if action=="status" { status();return }
        if action=="disable" { disable();return }
        guard action=="enable" else {fail(failure("未知操作"))}
        do {
            policyData=FileHandle.standardInput.readData(ofLength:32769)
            guard policyData.count<=32768 else {throw failure("策略过大")}
            let p=try JSONDecoder().decode(LeakPolicy.self,from:policyData);try p.validate()
            let request=OSSystemExtensionRequest.activationRequest(forExtensionWithIdentifier:extensionID,queue:.main)
            request.delegate=self;OSSystemExtensionManager.shared.submitRequest(request)
        } catch {fail(error)}
    }
    func requestNeedsUserApproval(_ request: OSSystemExtensionRequest) { /* final status stays unknown until completion */ }
    func request(_ request:OSSystemExtensionRequest,actionForReplacingExtension existing:OSSystemExtensionProperties,withExtension ext:OSSystemExtensionProperties) -> OSSystemExtensionRequest.ReplacementAction { .replace }
    func request(_ request:OSSystemExtensionRequest,didFailWithError error:Error) {fail(error)}
    func request(_ request:OSSystemExtensionRequest,didFinishWithResult result:OSSystemExtensionRequest.Result) {
        guard result == .completed else {emit(["state":"pending_restart","message":"系统扩展等待重新启动；尚未启用阻止"])}
        configure()
    }
    private func configure() {
        let dns=NEDNSProxyManager.shared()
        dns.loadFromPreferences { error in
            if let error {self.fail(error)}
            if dns.isEnabled, let id=dns.providerProtocol?.providerBundleIdentifier,id != self.extensionID {self.fail(failure("已有其他 DNS Proxy，未替换其设置"))}
            let oldProtocol=dns.providerProtocol, oldEnabled=dns.isEnabled
            let protocolConfig=NEDNSProxyProviderProtocol();protocolConfig.providerBundleIdentifier=self.extensionID;protocolConfig.providerConfiguration=["policy":self.policyData]
            dns.providerProtocol=protocolConfig;dns.localizedDescription="ClaudeGuard · DNS 经现有本机代理";dns.isEnabled=true
            dns.saveToPreferences { error in
                if let error {self.fail(error)}
                let filter=NEFilterManager.shared()
                filter.loadFromPreferences { error in
                    if let error {self.rollbackDNS(dns,oldProtocol,oldEnabled,error);return}
                    if filter.isEnabled, let id=filter.providerConfiguration?.filterDataProviderBundleIdentifier,id != self.extensionID {
                        self.rollbackDNS(dns,oldProtocol,oldEnabled,failure("已有其他过滤器，未替换其设置"));return
                    }
                    let config=NEFilterProviderConfiguration();config.filterSockets=true;config.filterPackets=false
                    config.filterDataProviderBundleIdentifier=self.extensionID;config.vendorConfiguration=["policy":self.policyData]
                    filter.providerConfiguration=config;filter.localizedDescription="ClaudeGuard · 仅允许已选应用访问本机代理";filter.isEnabled=true
                    filter.saveToPreferences { error in
                        if let error {self.rollbackDNS(dns,oldProtocol,oldEnabled,error);return}
                        self.status()
                    }
                }
            }
        }
    }
    private func rollbackDNS(_ manager:NEDNSProxyManager,_ config:NEDNSProxyProviderProtocol?,_ enabled:Bool,_ original:Error) {
        manager.providerProtocol=config;manager.isEnabled=enabled
        manager.saveToPreferences { error in
            if let error {self.fail(failure("配置失败，DNS 回滚也失败：\(error.localizedDescription)。请在系统设置中停用 ClaudeGuard DNS。"))}
            self.fail(original)
        }
    }
    private func disable() {
        let filter=NEFilterManager.shared()
        filter.loadFromPreferences { error in
            if let error {self.fail(error)}
            if let id=filter.providerConfiguration?.filterDataProviderBundleIdentifier,id != self.extensionID {self.fail(failure("过滤配置不属于 ClaudeGuard，未修改"))}
            filter.isEnabled=false
            filter.saveToPreferences { error in
                if let error {self.fail(error)}
                let dns=NEDNSProxyManager.shared()
                dns.loadFromPreferences { error in
                    if let error {self.fail(error)}
                    if let id=dns.providerProtocol?.providerBundleIdentifier,id != self.extensionID {self.fail(failure("DNS 配置不属于 ClaudeGuard，未修改"))}
                    dns.isEnabled=false
                    dns.saveToPreferences { error in if let error {self.fail(error)};self.status() }
                }
            }
        }
    }
    private func status() {
        let filter=NEFilterManager.shared()
        filter.loadFromPreferences { error in
            if let error {self.fail(error)}
            let dns=NEDNSProxyManager.shared()
            dns.loadFromPreferences { error in
                if let error {self.fail(error)}
                let f=filter.isEnabled && filter.providerConfiguration?.filterDataProviderBundleIdentifier==self.extensionID
                let d=dns.isEnabled && dns.providerProtocol?.providerBundleIdentifier==self.extensionID
                self.emit(["state":f && d ? "configured_unverified" : (f || d ? "partial" : "disabled"),"filterEnabled":f,"dnsEnabled":d,"message":f && d ? "系统已保存两项启用配置；需要重建应用连接并验证实际拦截，尚不等于保护验收通过" : (f || d ? "只启用了部分保护，不能视为完整保护；可停止后重试" : "网络阻止未启用")])
            }
        }
    }
}
func failure(_ text:String)->NSError {NSError(domain:"ClaudeGuard.Network",code:1,userInfo:[NSLocalizedDescriptionKey:text])}
