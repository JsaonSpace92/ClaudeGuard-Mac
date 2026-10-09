ClaudeGuard 随附的 Chromium WebRTC 保护组件

适用：支持 chrome.privacy.network.webRTCIPHandlingPolicy 的 Chrome、Edge、Brave。
1. 打开浏览器的扩展管理页，启用开发者模式。
2. 选择“加载已解压的扩展”，选择本目录。
3. 点击扩展图标，确认策略为 disable_non_proxied_udp，再运行 App 的浏览器测试。
浏览器必须由用户批准加载扩展，App 不能静默越过浏览器授权。
Safari / Firefox 不在这个 Chromium 组件的已验证范围内。
无痕窗口和已授权麦克风/摄像头的页面需单独验证。
恢复：在弹窗中选择“恢复浏览器原设置”，或移除此扩展。
本组件没有遥测、远程脚本、网页读取权限或网络请求。
仅约束 WebRTC 的浏览器策略，不能单独覆盖 DNS、IPv6 或桌面 App。
