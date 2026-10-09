# Net.Coffee 与白名单接入验收（2026-10-09）

## 实现

- 主界面四个入口：IP、DNS、WebRTC / UDP、Cloudflare；由 Rust 固定地址映射调用默认浏览器。
- 不把第三方页面加载到具有 Tauri IPC 权限的管理 WebView；网页结果在网页查看，不自动回传或改变守护判定。
- 主界面可编辑多个精确 IPv4 / IPv6 地址。加入当前出口先做新探测，仅填入编辑框，必须保存才生效。
- 名单单独保存时保留其他已保存设置及 armed；不采纳高级设置中未保存的编辑。保存成功后丢弃旧结果并重新检查。
- 停止守护不依赖正在编辑的名单是否有效。空名单不能开启守护。
- 移除主界面旧检测和实验性保护入口；停止启动时的自动本机风险扫描。保留旧命令及实验源码，不改变系统保护配置。

## 已验证

- Rust 36 项测试通过，包括固定 URL 映射、未知链接拒绝、IPv6 规范化与去重、无效名单拒绝和旧探测失效。
- `node scripts/test-coffee-ui.cjs` 使用真实前端脚本与隔离 DOM / IPC：四按钮映射；加入不自动保存；多 IP 和重复处理；保存保持端口与守护开关；探测或保存失败保持名单；停止守护忽略无效编辑内容。
- `node --check ui-mac/main.js`、`cargo fmt --check`、`cargo clippy --locked -- -D warnings`。
- release 打包及 codesign strict 验证通过。旧外置卷 target 含旧绝对路径和 AppleDouble 元数据，使用用户 Library/Caches 的独立构建目录。
- 更新 `/Applications/ClaudeGuard Mac.app` 前确认观察模式；备份旧 App，保留用户配置。实际 AX 检查显示四个网站按钮、自定义名单及新代理出口。
- 实际点击 Cloudflare 按钮后，App 提示已打开；Safari 标签列表出现对应 Cloudflare 页面。DNS 页面也在 Safari 中可见。

## 验收边界

第三方 DNS / WebRTC / Cloudflare 测试成功与否取决于网站及浏览器环境，本次没有宣称其检测结果全部通过。网站没有通过这一接入提供桌面应用流量归属或泄露阻止能力。实际生产白名单不通过测试脚本写入；保存行为使用隔离配置测试。

城市字段属于查询数据，不参与白名单判断。白名单按 IP 地址匹配。DNS 解析服务器地址不应加入应用出口白名单。
