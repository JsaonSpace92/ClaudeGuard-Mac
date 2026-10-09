use crate::{
    config::Config,
    guard,
    monitor::{self, Purpose, Shared},
};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::State;
type Sh<'a> = State<'a, Arc<Shared>>;
#[tauri::command]
pub fn get_state(sh: Sh) -> monitor::StatusSnapshot {
    sh.snapshot()
}
#[tauri::command]
pub fn get_config(sh: Sh) -> Config {
    sh.cfg.lock().unwrap().clone()
}
#[tauri::command]
pub fn set_config(sh: Sh, cfg: Config) -> Result<(), String> {
    sh.replace_config(cfg)
}
#[tauri::command]
pub fn recheck(sh: Sh) -> Result<(), String> {
    monitor::spawn_check(&sh, Purpose::Manual)
}
#[tauri::command]
pub fn launch(sh: Sh, id: String) -> Result<(), String> {
    if !guard::gui_agent_ids().contains(&id.as_str()) {
        return Err("未知应用".into());
    }
    monitor::spawn_check(&sh, Purpose::Launch(id))
}
#[tauri::command]
pub fn app_status() -> Vec<guard::AppInfo> {
    guard::app_status()
}
#[tauri::command]
pub fn recent_logs(sh: Sh) -> Vec<String> {
    sh.loglines.lock().unwrap().clone()
}
#[tauri::command]
pub fn open_data_folder() -> Result<(), String> {
    let dir = crate::config::config_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::process::Command::new("/usr/bin/open")
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn coffee_url(target: &str) -> Result<&'static str, String> {
    match target {
        "ip" => Ok("https://ip.net.coffee/"),
        "dns" => Ok("https://ip.net.coffee/dns/"),
        "webrtc" => Ok("https://ip.net.coffee/webrtc/"),
        "cloudflare" => Ok("https://ip.net.coffee/cloudflare/"),
        _ => Err("未知的网站检测项目".into()),
    }
}

#[tauri::command]
pub fn open_coffee_test(target: String) -> Result<(), String> {
    std::process::Command::new("/usr/bin/open")
        .arg(coffee_url(&target)?)
        .status()
        .map_err(|_| "无法打开默认浏览器".to_string())?
        .success()
        .then_some(())
        .ok_or_else(|| "无法打开默认浏览器".into())
}

#[tauri::command]
pub async fn current_proxy_exit(sh: Sh<'_>) -> Result<serde_json::Value, String> {
    let shared = sh.inner().clone();
    let (cfg, revision) = probe_config(&shared);
    let result = tauri::async_runtime::spawn_blocking(move || crate::dashboard::proxy_exit(&cfg))
        .await
        .map_err(|_| "代理出口检测未完成".to_string())?;
    ensure_current_probe(&shared, revision)?;
    Ok(result)
}
#[tauri::command]
pub fn set_autostart(sh: Sh, enabled: bool) -> Result<(), String> {
    let previous = sh.cfg.lock().unwrap().auto_start_with_system;
    crate::install::set_autostart(enabled)?;
    let mut cfg = sh.cfg.lock().unwrap().clone();
    cfg.auto_start_with_system = enabled;
    if let Err(e) = sh.replace_config(cfg) {
        let _ = crate::install::set_autostart(previous);
        return Err(e);
    }
    Ok(())
}

#[tauri::command]
pub async fn check_risks(sh: Sh<'_>, deep: bool) -> Result<crate::risks::RiskReport, String> {
    let shared = sh.inner().clone();
    tauri::async_runtime::spawn_blocking(move || monitor::run_risk_check(&shared, deep))
        .await
        .map_err(|_| "风险检查无法完成".to_string())?
}

#[tauri::command]
pub fn browser_test(sh: Sh) -> Result<String, String> {
    // Keep the config lock until the session exists so a concurrent save
    // always invalidates this session rather than racing ahead of its start.
    let cfg = sh.cfg.lock().unwrap();
    let url = format!("{}#run", sh.browser_tests.start(cfg.clone())?);
    drop(cfg);
    std::process::Command::new("/usr/bin/open")
        .arg(&url)
        .spawn()
        .map_err(|_| "无法打开默认浏览器")?;
    Ok(url)
}
#[tauri::command]
pub fn browser_report(sh: Sh) -> Option<serde_json::Value> {
    sh.browser_tests.report()
}
#[tauri::command]
pub async fn udp_test(sh: Sh<'_>) -> Result<crate::leaks::UdpReport, String> {
    let shared = sh.inner().clone();
    let (cfg, revision) = probe_config(&shared);
    let result = tauri::async_runtime::spawn_blocking(move || crate::leaks::probe_udp(&cfg))
        .await
        .map_err(|_| "UDP 探测未完成".to_string())?;
    ensure_current_probe(&shared, revision)?;
    Ok(result)
}
#[tauri::command]
pub fn network_apps() -> Vec<crate::network::Application> {
    crate::network::applications()
}
#[tauri::command]
pub async fn network_control(
    action: String,
    policy: Option<crate::network::Policy>,
) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || crate::network::control(&action, policy))
        .await
        .map_err(|_| "网络操作未完成".to_string())?
}

#[tauri::command]
pub fn browser_protection_files() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|_| "无法定位 App")?;
    let resources = exe
        .parent()
        .and_then(|p| p.parent())
        .ok_or("无法定位 App")?
        .join("Resources/BrowserProtection");
    if !resources.join("manifest.json").is_file() {
        return Err("请使用打包后的 App 打开浏览器组件".into());
    }
    std::process::Command::new("/usr/bin/open")
        .arg(resources)
        .spawn()
        .map(|_| ())
        .map_err(|_| "无法打开浏览器组件".into())
}

#[tauri::command]
pub async fn system_dns_test(sh: Sh<'_>) -> Result<serde_json::Value, String> {
    let shared = sh.inner().clone();
    let (cfg, revision) = probe_config(&shared);
    let result = tauri::async_runtime::spawn_blocking(move || crate::leaks::probe_system_dns(&cfg))
        .await
        .map_err(|_| "系统 DNS 检测未完成".to_string())?;
    ensure_current_probe(&shared, revision)?;
    Ok(result)
}

fn probe_config(shared: &Shared) -> (Config, u64) {
    let cfg = shared.cfg.lock().unwrap();
    (cfg.clone(), shared.revision.load(Ordering::SeqCst))
}

#[tauri::command]
pub async fn dashboard_probe(sh: Sh<'_>) -> Result<serde_json::Value, String> {
    let shared = sh.inner().clone();
    let (cfg, revision) = probe_config(&shared);
    let result = tauri::async_runtime::spawn_blocking(move || crate::dashboard::inspect(&cfg))
        .await
        .map_err(|_| "出口概览检测未完成".to_string())?;
    ensure_current_probe(&shared, revision)?;
    Ok(result)
}

#[tauri::command]
pub async fn system_environment() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(|| crate::network::control("environment", None))
        .await
        .map_err(|_| "系统环境读取未完成".to_string())?
}

fn ensure_current_probe(shared: &Shared, revision: u64) -> Result<(), String> {
    if shared.stop.load(Ordering::SeqCst) || shared.revision.load(Ordering::SeqCst) != revision {
        return Err("检测途中配置已改变或 App 正在退出，旧结果已丢弃，请重新检测".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coffee_links_are_fixed_and_reject_arbitrary_urls() {
        for (target, suffix) in [
            ("ip", ""),
            ("dns", "dns/"),
            ("webrtc", "webrtc/"),
            ("cloudflare", "cloudflare/"),
        ] {
            assert_eq!(
                coffee_url(target).unwrap(),
                format!("https://ip.net.coffee/{suffix}")
            );
        }
        assert!(coffee_url("https://example.com").is_err());
        assert!(coffee_url("--args").is_err());
    }

    #[test]
    fn config_change_and_shutdown_discard_inflight_probe() {
        let shared = Shared::new(Config::default());
        let (_, revision) = probe_config(&shared);
        assert!(ensure_current_probe(&shared, revision).is_ok());
        shared.revision.fetch_add(1, Ordering::SeqCst);
        assert!(ensure_current_probe(&shared, revision).is_err());
        let (_, revision) = probe_config(&shared);
        shared.stop.store(true, Ordering::SeqCst);
        assert!(ensure_current_probe(&shared, revision).is_err());
    }
}
