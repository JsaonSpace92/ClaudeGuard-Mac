# ClaudeGuard Mac

macOS 菜单栏出口守护器：通过现有 Clash HTTP/Mixed 代理检测公网出口 IP，在出口不符合白名单或代理失效时，关闭选中的 AI 桌面应用。

基于 [daha1216/ClaudeGuard](https://github.com/daha1216/ClaudeGuard) 的 macOS 改造版。当前版本 **2.11.2**，使用 Rust + Tauri 2。

## 功能

- 支持 Claude Desktop、ChatGPT Desktop、Antigravity Desktop。
- 精确匹配一个或多个固定公网出口 IP。
- 同时通过 ipify 和 ipinfo 检测，支持 Clash 的 HTTP/Mixed 入口和 TUN 场景。
- 不匹配的出口 IP 或本机代理端口失效，在本轮检测中触发关闭；两个公网探测都失败时按连续失败阈值处理。
- 菜单栏常驻、登录自启、手动检测、通过检测后启动应用。
- 关闭管理窗口会释放 WebView，后台检测继续运行；重新打开恢复状态与日志。
- 首次运行默认是观察模式，显式开启后才会关闭应用。
- IPv6 / DNS 风险检查：每分钟读取本机地址、路由和 DNS 信息；支持手动深度检测。这些结果只提示，不触发关闭。

## 使用

1. 构建下方的 `.app`，把 `dist/ClaudeGuard Mac.app` 拖入“应用程序”后启动。
2. 填写 Clash 的 HTTP/Mixed 端口，默认 `7897`；请以自己的 Clash 设置为准。
3. 填入代理服务商提供的固定**公网出口 IP**。这不是本机地址，也不一定是代理服务器连接地址。
4. 在观察模式检测，确认探测流量和目标应用都使用同一固定出口。
5. 勾选需要保护的应用，点击“开启守护”。
6. 点击菜单栏盾牌可打开管理窗口，右键打开菜单。退出守护器会停止监测。

恢复网络后不会自动重新启动应用。关闭异常应用会中断进行中的任务。已开启的守护状态会保存，下次启动按保存的设置继续运行。

应用通过 `/Applications` 和 `~/Applications` 中的应用包名称及 Bundle ID 识别；找不到的应用会显示提示。

## IPv6 / DNS 风险检查

管理窗口新增风险检查区域。轻量检查每分钟运行，不发送外部探测请求。手动“深度检测”通过现有代理查询 AAAA，然后通过系统路由测试 IPv6 出口；同时向最多三个系统 DNS 地址发送随机 `.invalid` 域名的 UDP 查询，观察应答与路由。系统 TUN 仍可能接管这些探测。

结果区分“发现风险”“尚未核验”“本次未发现”。非回环 DNS 地址、物理网卡路由属于需要核验的风险迹象，不能仅凭它们断言已泄漏。DNS 深度检测也不能证明递归解析器的最终公网出口。IPv6 单个目标连接失败不能证明所有 IPv6 流量都被阻止。深度结果保留检测时间，切换网络后请重测。

这两项检查不改变网络设置、不进入异常关闭判断。WebRTC 和浏览器保护暂未实现。

## 限制

本工具采用**检测后关闭应用**的方式，存在轮询和网络超时窗口，不保证零 IP 泄漏。

- 默认两轮之间等待 2 秒，单轮探测另需耗时；默认公网超时 2 秒、连续失败阈值 2 次。
- Clash 可能把探测网站与目标应用分流到不同出口。探测通过不能证明每条应用连接都使用该出口。
- 只关闭选中 `.app` 包内的进程。浏览器、独立终端、外部 MCP 进程不在保护范围内。
- 不修改 Clash 配置，不设置系统防火墙，不包含独立代理内核。
- 登录自启通过用户 LaunchAgent 实现，下次登录生效。移动 App 后需重新设置登录自启。
- 当前构建使用本地 ad-hoc 签名，未进行 Developer ID 公证。

## 本地构建

需要 macOS、Rust 稳定版（含 Cargo / rustfmt / clippy）、Python 3、Node.js（仅用于 JS 语法检查），以及 Xcode Command Line Tools。

已在 Apple Silicon macOS 上验证，Intel Mac 和较旧系统尚未实测。脚本为当前主机架构构建，不生成 Universal Binary。

```sh
cargo fmt --check
cargo clippy --locked -- -D warnings
cargo test --locked
node --check ui-mac/main.js
python3 scripts/package-macos.py
```

构建产物：`dist/ClaudeGuard Mac.app`。修改前端后需要重新构建，因为静态页面在编译时嵌入。

重新生成原创应用图标（使用系统 Swift / AppKit）：

```sh
swift scripts/generate-app-icon.swift assets/icon.png
```

重新生成菜单栏图标：

```sh
python3 scripts/generate-tray-icon.py
```

只读检测（不会关闭应用）：

```sh
./target/release/claude-guard --check
./target/release/claude-guard --list-apps
./target/release/claude-guard --risk-check
./target/release/claude-guard --risk-deep
```

`--check` 输出 JSON，检测通过退出码为 0，失败为 2。`--tray` 启动后台菜单栏模式。

## 数据与隐私

配置与日志保存于：

```text
~/Library/Application Support/ClaudeGuard Mac/
```

配置为 `config.json`，日志为 `guard.log`，超过 1 MB 时轮换为一份备份。登录自启文件位于 `~/Library/LaunchAgents/local.claudeguard.mac.plist`。

`CG_DATA_DIR` 可指定隔离测试目录。固定出口 IP、配置、日志、签名证书和构建产物均不应提交到源码仓库。

启用检测会通过配置的本机代理请求 ipify 和 ipinfo，探测服务能够看到该请求的出口 IP。手动深度检测另会请求 Cloudflare DoH 与 ipify IPv6 服务，它们能够看到对应请求的出口 IP；系统 DNS 会收到随机测试域名。风险结果不记录观察到的公网地址。项目没有单独的遥测上传功能。

## 项目结构

```text
src/           检测、配置、后台调度、应用关闭与 macOS 集成
ui-mac/        管理界面（原生静态 HTML / CSS / JS）
assets/        应用与菜单栏图标
capabilities/  Tauri 窗口权限
scripts/       本地打包与图标生成
docs/         验证说明与变更记录
```

详见 [验证说明](docs/VALIDATION.md)、[变更记录](CHANGELOG.md) 和 [来源与许可说明](NOTICE.md)。
