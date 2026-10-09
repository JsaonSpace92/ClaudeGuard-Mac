# DNS / WebRTC / UDP / IPv6 检测与阻止：接入与验收说明

当前状态：检测界面、浏览器测试页、系统 DNS / UDP 主动探测及实验性网络扩展源码已接入。
**没有安装或启用网络扩展，尚未完成系统级阻止验收。不要将当前 App 视为已具备完整防泄露保护。**

2026-10-06 本机检查：0 个可用代码签名身份；现有打包流程采用 ad-hoc 签名。系统扩展需要 Developer ID 和相应 provisioning profiles。编译成功不是安装成功，更不是已拦截真实流量。

## 可行性与范围

浏览器和普通桌面 App 的标准 TCP/UDP 流均可由 macOS Network Extension 观察、允许或丢弃。Tauri 管理窗口继续作为控制界面，系统扩展为独立原生进程，关闭窗口不会结束系统扩展。

- DNS：`NEDNSProxyProvider` 接收系统交付的 DNS TCP/UDP 查询，将 DNS wire message 用 HTTPS 经已有本机 HTTP/Mixed 代理发往 DoH 服务。代理不可达、拒绝、超时或上游返回无效数据时终止请求，没有直连备用路径。使用数值型 DoH 地址避免解析 DoH 主机名造成自递归。
- WebRTC/UDP：`NEFilterDataProvider` 对已选应用仅放行指定本机代理的 TCP 端口，拒绝其他 UDP（包括非标准 STUN 端口、QUIC）和直接 TCP 连接。保护是端口无关的严格入口限制；不是把 UDP 自动转发到链式代理。
- IPv6：同样拒绝已选应用的其他连接，包括公网 IPv6。可允许已配置的 `::1` 本机代理入口。
- 浏览器地址暴露：网络拦截不能阻止浏览器 API 在本地生成候选地址，因此随 App 附带 Chromium privacy API 组件，将 WebRTC 策略设置为 `disable_non_proxied_udp`。浏览器必须由用户批准加载，不能由 App 静默安装。需分别核验普通窗口、无痕窗口、已有媒体权限的页面。Safari/Firefox 不属于此 Chromium 组件的已验证范围。

系统 DNS 可能由 mDNSResponder 代发，不能把所有查询可靠地归属到单个 App，因此本实现启用时明确要求确认“系统 DNS 范围”。应用内置 DoH/DoT/DoQ 的其他直接连接由严格入口策略限制；经允许的本机代理发送的连接仍由现有 Clash 路由决定。

## 与 Clash 的关系

本实现不写 Clash YAML、不访问 Clash 修改接口、不切换模式、不修改其规则或启动状态，也不写系统代理地址、系统 DNS 地址或 PF 规则。

系统扩展启用属于新增 macOS 网络处理组件，会影响数据流。严格模式只允许应用显式访问本机代理；只依赖 TUN、未使用显式代理的应用可能无法联网。UDP 语音视频可能不可用。它不会把被阻止的连接偷偷改为直连。

系统扩展策略独立保存；在 App 修改代理端口后，须重新启用连接阻止策略才能更新入口。

放行本机代理不能证明 Clash 内部所有分流均经过指定静态 IP；原有出口守护仍只证明探测连接的出口。链式代理的 bootstrap DNS、Clash 的内部 DNS 和 TUN 拦截顺序必须在目标机器验证，以排除递归解析、无法联网或未覆盖路径。不能以“共存接口允许安装”推断运行时兼容。

## 本次研究的 GitHub 项目

| 项目 | 用途与选择 |
|---|---|
| https://github.com/objective-see/LuLu | 成熟 macOS 按应用出站防火墙，GPL-3.0。参考 Network Extension 分层和应用身份设计，没有复制其 GPL 代码。 |
| https://github.com/AdguardTeam/DnsLibs | Apache-2.0，原生 DNS proxy 与 macOS NEDNSProxyProvider 适配。参考其双协议 DNS 流处理说明；暂未引入整套 C++ 构建。 |
| https://github.com/AdguardTeam/dnsproxy | 可用的 Go DNS 转发器；单独运行不会自动接管全部 App DNS，仍需系统集成。 |
| https://github.com/webrtc/samples | BSD-3-Clause；参考 Trickle ICE 的标准浏览器 API 用法，实现轻量测试页。 |
| https://github.com/macvk/dnsleaktest | MIT；沿用 bash.ws 随机域名检测机制，实现 Rust 后端及浏览器触发器，不照搬 ASN 自动判定。 |
| https://github.com/dlinbernard/webrtc-control | 浏览器 WebRTC 控制方向的参考；不覆盖桌面 App。当前随附组件是基于官方 privacy API 新写的最小实现，未复制此项目代码。 |
| https://github.com/mullvad/mullvadvpn-app | 自有 VPN 与防火墙整合较深，不适合在“不改变既有 Clash”的前提下整体移植。 |

所有新增实现均基于公开系统 API / 协议独立编写，未导入上述项目源文件，也未改变本项目原有许可声明。

官方依据：
- https://developer.apple.com/documentation/technotes/tn3134-network-extension-provider-deployment
- https://developer.apple.com/documentation/networkextension/nefilterflow/sourceappaudittoken
- https://developer.apple.com/documentation/networkextension/nednsproxyproviderprotocol/providerconfiguration
- https://developer.chrome.com/docs/extensions/reference/api/privacy
- https://github.com/AdguardTeam/DnsLibs/blob/master/docs/dns-proxy-provider.md

## 代码位置

- `src/leaks.rs`：STUN 编解码、系统 DNS 公网解析器采样、仅回环监听的短期浏览器会话。
- `ui-mac/browser-test.html`：当前浏览器 HTTP、IPv6、WebRTC 候选与 DNS 测试。
- `browser-extension/`：随 App 打包的 Chromium WebRTC 控制组件，提供恢复设置按钮。
- `src/network.rs`：应用选择、策略校验、原生控制器调用；禁止把 configured 当成 verified。
- `network-extension/Policy.swift`：明确的应用包边界与精确本机代理入口匹配。
- `network-extension/FilterProvider.swift`：新流拦截；以 audit token 获取源应用可执行路径，macOS 13 起同时核对实际建连进程，任一匹配已选应用即受限制。两类身份都无法读取时仍有未覆盖范围。
- `network-extension/DNSProvider.swift`：UDP 与 TCP DNS 流、TCP 长度帧、生命周期与并发上限。
- `network-extension/DNSOverProxy.swift`：强制经本机代理的 DoH 传输，拒绝重定向。
- `network-extension/Controller.swift`：安装请求、DNS/过滤启停、部分失败回滚及明确状态。

检测结果只用于报告，不进入原有关闭应用判断。浏览器报告来自浏览器上报，不作为可信拦截状态证明；不写入日志或配置。刷新配置会使浏览器会话与结果失效。检测请求会访问 ipify、Google/Cloudflare STUN、bash.ws，服务端能够看到相应出口。

## 构建

普通构建：`python3 scripts/package-macos.py`。包含检测功能、浏览器组件与原生控制器；不包含已获授权的网络扩展，启用按钮保持不可用。

仅编译原生源码：`python3 scripts/build-network-extension.py`。不安装、不启用。

本地协议与阻止策略测试：`python3 scripts/test-network-extension.py`。只用回环模拟代理，不故意断开现有代理。

准备正式签名构建（需要用户自己的证书与两个有效描述文件）：

```sh
python3 scripts/build-network-extension.py \
  --app 'dist/ClaudeGuard Mac.app' \
  --identity 'Developer ID Application: YOUR IDENTITY' \
  --app-profile /absolute/path/host.provisionprofile \
  --extension-profile /absolute/path/extension.provisionprofile
```

上述命令只打包签名，不启用。主 App ID 为 `local.claudeguard.mac`，扩展为 `local.claudeguard.mac.network`。描述文件必须授权 content-filter-provider-systemextension 和 dns-proxy-systemextension；主 App 还需要 system-extension.install。正式分发还需验证公证、目标架构与系统版本。

## 尚未完成的端到端验收

必须在正式签名扩展已加载、用户明确选择目标应用后逐项完成：

1. Chrome/Edge/Brave、Safari，以及 Claude 桌面 App 主进程和 Helper/WebKit 网络进程的身份归属。无法读取 audit identity 的流当前放行，属于明确的未覆盖范围，不能宣称全量防护。
2. 目标应用正常代理请求成功；任意端口 UDP、直接 TCP、IPv6 连接被拒绝。排除系统扩展开启前已存在的连接继续传输。需要用户主动重建连接，本实现不自动关闭真实 App。
3. 真实 DNS UDP/TCP 查询只经代理 DoH 返回；本机模拟代理拒绝、超时、DNS 服务拒绝时无直接 DNS fallback；系统加密 DNS、应用自带 DoH/DoQ 单独验证。
4. 在授权的隔离测试环境验证断网、代理退出、网络切换、睡眠恢复和扩展异常退出行为。当前没有证明扩展崩溃/失活时 fail-closed。
5. 浏览器候选地址、媒体权限与无痕模式分别验证，不能只依赖没有 srflx 候选。
6. 抓包同时观察物理网卡与 TUN，结合目标进程元数据确认流量去向。当前未实现 App 内完整 PKTAP 抓包与实时逐应用泄露日志。
7. 测试停止两个 provider、只启用其中一个、安装等待批准、配置失败回滚；不把 isEnabled 当作扩展运行证据。

因此当前交付是可构建的集成与实验性系统扩展源码，不是已验收的防泄露成品。剩余核心条件：签名/配置授权、真实进程覆盖、失败关闭行为及与用户链式代理的运行时验证。
