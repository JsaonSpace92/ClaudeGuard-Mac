# ClaudeGuard Mac 维护约定

此仓库是 macOS 版。核心流程：显式通过现有本机代理检查出口，按配置关闭选中的应用包内进程，菜单栏常驻管理。

## 范围

- 不擅自修改 Clash 配置、系统防火墙或代理规则。
- 保留观察模式、显式开启守护、精确 IP 匹配和配置迁移兼容性。
- 不用通用 node/python 进程名终止进程；按应用包路径匹配，并复核 PID 与启动时间。
- 配置变化后丢弃过期检测结果。停止守护必须可用，不依赖表单是否填写完整。
- 后台状态独立于 WebView。关闭管理窗口后继续检测，重新打开恢复状态。
- 不在 Tauri 主线程执行网络探测和异常关闭。

## 文件

- `src/main.rs`：应用入口、窗口与菜单栏生命周期。
- `src/monitor.rs`：单线程调度、请求队列、状态与异常关闭编排。
- `src/checks.rs`：显式代理、端口与公网出口检测。
- `src/risks.rs`：独立 IPv6 / DNS 风险诊断，结果不得进入关闭判断。
- `src/guard.rs`：应用发现、进程身份校验、关闭与日志。
- `src/config.rs`：配置读写和迁移。
- `src/install.rs`：用户级登录自启。
- `src/ipc.rs`：管理界面命令。
- `ui-mac/`：静态管理界面，经典脚本保持 IIFE，避免全局词法变量冲突。

## 本地验证

```sh
cargo fmt --check
cargo clippy --locked -- -D warnings
cargo test --locked
node --check ui-mac/main.js
python3 scripts/package-macos.py
```

测试只终止临时编译的测试应用，不关闭用户真实应用、不故意断开用户代理。使用 `CG_DATA_DIR` 隔离手动测试配置。

前端在编译时嵌入，修改后需重新编译。打包版本应读取 Cargo.toml，不另维护重复版本号。

不要提交本机出口 IP、配置、日志、凭据、证书、target、dist 或生成的 schema。不要未经用户要求启用 CI、推送代码、创建 Release 或更改项目许可。当前仓库不包含自动触发工作流。
