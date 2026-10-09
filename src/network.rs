//! Embedded macOS Network Extension controller. Configured != verified active.
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};
static OPERATION: Mutex<()> = Mutex::new(());
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    pub proxy_host: String,
    pub proxy_port: u16,
    pub application_roots: Vec<String>,
    pub acknowledge_system_dns: bool,
}
#[derive(Serialize)]
pub struct Application {
    pub name: String,
    pub path: String,
}
fn helper() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let path = exe.parent()?.join("claudeguard-network");
    path.is_file().then_some(path)
}
pub fn applications() -> Vec<Application> {
    let mut bases = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        bases.push(PathBuf::from(home).join("Applications"));
    }
    let mut apps = Vec::new();
    for base in bases {
        let Ok(entries) = std::fs::read_dir(base) else {
            continue;
        };
        for entry in entries.flatten().take(512) {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "app")
                && path.join("Contents/Info.plist").is_file()
            {
                if let Ok(path) = path.canonicalize() {
                    let name = path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    // Guard and proxies must not be included in their own strict filter.
                    if ["claudeguard", "clash", "mihomo"]
                        .iter()
                        .any(|word| name.to_lowercase().contains(word))
                    {
                        continue;
                    }
                    apps.push(Application {
                        name,
                        path: path.to_string_lossy().to_string(),
                    });
                }
            }
        }
    }
    apps.sort_by(|a, b| a.name.cmp(&b.name));
    apps.dedup_by(|a, b| a.path == b.path);
    apps
}
fn validate(policy: &Policy) -> Result<(), String> {
    if !matches!(policy.proxy_host.as_str(), "127.0.0.1" | "::1")
        || policy.proxy_port == 0
        || !policy.acknowledge_system_dns
    {
        return Err("需确认本机代理入口，以及 DNS 保护覆盖系统解析的范围".into());
    }
    let installed = applications();
    if policy.application_roots.is_empty()
        || policy.application_roots.len() > 32
        || policy
            .application_roots
            .iter()
            .any(|p| !installed.iter().any(|a| a.path == *p))
    {
        return Err("请从已安装应用中选择 1–32 个保护对象".into());
    }
    Ok(())
}
pub fn control(action: &str, policy: Option<Policy>) -> Result<serde_json::Value, String> {
    // Environment reads do not touch network preferences and may run alongside
    // a status/enable operation without making the dashboard spuriously fail.
    let _lock = if action == "environment" {
        None
    } else {
        Some(
            OPERATION
                .try_lock()
                .map_err(|_| "网络扩展操作进行中，请稍后重试")?,
        )
    };
    if !matches!(action, "status" | "enable" | "disable" | "environment") {
        return Err("未知网络操作".into());
    }
    let body = if action == "enable" {
        let p = policy.ok_or("缺少保护策略")?;
        validate(&p)?;
        Some(serde_json::to_vec(&p).map_err(|_| "策略编码失败")?)
    } else {
        None
    };
    let Some(path) = helper() else {
        return Ok(
            serde_json::json!({"state":"unavailable","message":"当前构建未包含已签名网络扩展。检测可使用；连接阻止尚未生效。"}),
        );
    };
    let mut child = Command::new(path)
        .arg(action)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "无法启动网络控制模块")?;
    if let Some(body) = body {
        if let Some(mut input) = child.stdin.take() {
            input.write_all(&body).map_err(|_| "无法传递策略")?;
        }
    } else {
        drop(child.stdin.take());
    }
    let deadline = Instant::now() + Duration::from_secs(55);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("系统操作尚未确认完成，请刷新状态；不要假定保护已经生效".into());
            }
        }
    }
    let mut output = String::new();
    if let Some(stdout) = child.stdout.take() {
        stdout
            .take(16384)
            .read_to_string(&mut output)
            .map_err(|_| "无法读取网络状态")?;
    }
    serde_json::from_str(&output).map_err(|_| "网络模块未返回有效状态".into())
}
