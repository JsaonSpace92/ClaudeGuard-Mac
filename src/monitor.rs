//! Original background-check pattern, with explicit arming and stale-result rejection.
use crate::risks::{self, RiskReport};
use crate::{
    checks::{self, CheckOutcome},
    config::Config,
    guard::{self, TripResult},
};
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Condvar, Mutex,
};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

#[derive(Clone, PartialEq, Eq)]
pub enum Purpose {
    Monitor,
    Manual,
    Launch(String),
}
pub struct Shared {
    pub cfg: Mutex<Config>,
    pub last: Mutex<Option<CheckOutcome>>,
    pub tripped: Mutex<Option<TripResult>>,
    pub loglines: Mutex<Vec<String>>,
    pub busy: AtomicBool,
    pub stop: AtomicBool,
    pub revision: AtomicU64,
    pub risks: Mutex<Option<RiskReport>>,
    risk_busy: AtomicBool,
    failures: Mutex<u32>,
    app: Mutex<Option<tauri::AppHandle>>,
    wake: Condvar,
    pending: Mutex<VecDeque<Purpose>>,
    last_checked_at: AtomicU64,
    last_duration_ms: AtomicU64,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusSnapshot {
    pub checking: bool,
    pub armed: bool,
    pub last: Option<CheckOutcome>,
    pub tripped: Option<TripResult>,
    pub failures: u32,
    pub version: &'static str,
    pub last_checked_at: u64,
    pub last_duration_ms: u64,
    pub risks: Option<RiskReport>,
    pub risk_checking: bool,
}
impl Shared {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg: Mutex::new(cfg),
            last: Mutex::new(None),
            tripped: Mutex::new(None),
            loglines: Mutex::new(Vec::new()),
            busy: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            revision: AtomicU64::new(0),
            risks: Mutex::new(None),
            risk_busy: AtomicBool::new(false),
            failures: Mutex::new(0),
            app: Mutex::new(None),
            wake: Condvar::new(),
            pending: Mutex::new(VecDeque::new()),
            last_checked_at: AtomicU64::new(0),
            last_duration_ms: AtomicU64::new(0),
        }
    }
    pub fn attach(&self, app: tauri::AppHandle) {
        *self.app.lock().unwrap() = Some(app);
    }
    pub fn log(&self, msg: impl Into<String>) {
        let msg = msg.into();
        #[cfg(not(test))]
        guard::log_line(&msg);
        let mut lines = self.loglines.lock().unwrap();
        lines.push(msg);
        if lines.len() > 200 {
            lines.remove(0);
        }
        if let Some(app) = self.app.lock().unwrap().as_ref() {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.emit("guard://log", lines.clone());
            }
        }
    }
    pub fn snapshot(&self) -> StatusSnapshot {
        StatusSnapshot {
            checking: self.busy.load(Ordering::SeqCst),
            armed: self.cfg.lock().unwrap().armed,
            last: self.last.lock().unwrap().clone(),
            tripped: self.tripped.lock().unwrap().clone(),
            failures: *self.failures.lock().unwrap(),
            version: env!("CARGO_PKG_VERSION"),
            last_checked_at: self.last_checked_at.load(Ordering::SeqCst),
            last_duration_ms: self.last_duration_ms.load(Ordering::SeqCst),
            risks: self.risks.lock().unwrap().clone(),
            risk_checking: self.risk_busy.load(Ordering::SeqCst),
        }
    }
    pub fn emit_status(&self) {
        // Release the app lock before snapshot takes configuration locks.
        let window = self
            .app
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|app| app.get_webview_window("main"));
        if let Some(window) = window {
            let _ = window.emit("guard://status", self.snapshot());
        }
    }
    pub fn wake_monitor(&self) {
        let _pending = self.pending.lock().unwrap();
        self.wake.notify_one();
    }
    pub fn request_check(&self, purpose: Purpose) -> Result<(), String> {
        let mut pending = self.pending.lock().unwrap();
        if self.stop.load(Ordering::SeqCst) {
            return Err("守护器正在退出".into());
        }
        if !pending.contains(&purpose) {
            if pending.len() >= 8 {
                return Err("等待中的操作过多，请稍后重试".into());
            }
            pending.push_back(purpose);
        }
        self.wake.notify_one();
        Ok(())
    }
    pub fn replace_config(&self, mut cfg: Config) -> Result<(), String> {
        cfg.migrate();
        validate_config(&cfg)?;
        cfg.egress_region.clear();
        cfg.quarantine_on_fail = false;
        cfg.first_run = cfg.allowed_ips.is_empty();
        let mut current = self.cfg.lock().unwrap();
        cfg.save().map_err(|e| e.to_string())?;
        *current = cfg;
        self.revision.fetch_add(1, Ordering::SeqCst);
        *self.risks.lock().unwrap() = None;
        *self.failures.lock().unwrap() = 0;
        *self.last.lock().unwrap() = None;
        *self.tripped.lock().unwrap() = None;
        self.last_checked_at.store(0, Ordering::SeqCst);
        self.last_duration_ms.store(0, Ordering::SeqCst);
        drop(current);
        self.request_check(Purpose::Manual)?;
        self.emit_status();
        Ok(())
    }
}

pub fn validate_config(cfg: &Config) -> Result<(), String> {
    if !matches!(cfg.proxy_host.as_str(), "127.0.0.1" | "localhost" | "::1") {
        return Err("仅支持本机代理入口".into());
    }
    if cfg.proxy_port == 0 {
        return Err("代理端口必须为 1–65535".into());
    }
    if !(1..=60).contains(&cfg.check_interval_secs) {
        return Err("检测间隔必须为 1–60 秒".into());
    }
    if !(1..=5).contains(&cfg.failure_threshold) {
        return Err("探测失败次数必须为 1–5".into());
    }
    if !(1..=10).contains(&cfg.probe_timeout_secs) {
        return Err("探测超时必须为 1–10 秒".into());
    }
    if cfg
        .allowed_ips
        .iter()
        .any(|ip| ip.parse::<std::net::IpAddr>().is_err())
    {
        return Err("白名单中存在无效 IP".into());
    }
    if cfg.armed
        && (cfg.allowed_ips.is_empty() || cfg.guarded_agents.is_empty() || !cfg.kill_on_fail)
    {
        return Err("开启守护需要配置出口 IP、至少选择一个应用并开启异常关闭".into());
    }
    Ok(())
}

pub fn should_trip(cfg: &Config, passed: bool, immediate: bool, failures: u32) -> bool {
    cfg.armed
        && !cfg.first_run
        && cfg.kill_on_fail
        && !cfg.allowed_ips.is_empty()
        && !passed
        && (immediate || failures >= cfg.failure_threshold)
}

pub fn run_check(sh: &Arc<Shared>, purpose: Purpose) {
    if sh.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    let (cfg, revision) = {
        let cfg = sh.cfg.lock().unwrap();
        (cfg.clone(), sh.revision.load(Ordering::SeqCst))
    };
    sh.emit_status();
    let started = Instant::now();
    let out = checks::run_checks(&cfg, |m| sh.log(m));
    let duration_ms = started.elapsed().as_millis() as u64;
    // Holding cfg across action makes disabling/config edits synchronize with termination.
    let current = sh.cfg.lock().unwrap();
    if revision != sh.revision.load(Ordering::SeqCst) || sh.stop.load(Ordering::SeqCst) {
        drop(current);
        sh.log("配置已改变：丢弃旧检测结果");
        sh.busy.store(false, Ordering::SeqCst);
        sh.emit_status();
        return;
    }
    let failures = {
        let mut n = sh.failures.lock().unwrap();
        *n = if out.passed { 0 } else { n.saturating_add(1) };
        *n
    };
    let immediate = out.confirmed_mismatch
        || (out.port.state == checks::StepState::Fail && !cfg.allowed_ips.is_empty());
    let trip = should_trip(&cfg, out.passed, immediate, failures);
    let previous_pass = sh.last.lock().unwrap().as_ref().map(|x| x.passed);
    if previous_pass != Some(out.passed) || !matches!(purpose, Purpose::Monitor) {
        sh.log(if out.passed {
            format!("检测通过：{}", out.egress_ip.as_deref().unwrap_or(""))
        } else {
            format!("检测失败：{}（连续 {failures} 次）", out.reason)
        });
    }
    if trip {
        let result = guard::trip(&cfg);
        let previous = sh.tripped.lock().unwrap().clone();
        if previous.as_ref() != Some(&result) || result.killed > 0 {
            sh.log(format!(
                "已触发异常关闭：终止 {} 个进程，仍运行 {} 个",
                result.killed, result.remaining
            ));
        }
        // Always replace the latest result so old remaining-process counts cannot linger.
        *sh.tripped.lock().unwrap() = Some(result);
    } else if out.passed {
        *sh.tripped.lock().unwrap() = None;
    }
    if let Purpose::Launch(ref id) = purpose {
        if out.passed && cfg.armed && cfg.guarded_agents.contains(id) {
            match guard::launch_agent(id) {
                Ok(()) => sh.log(format!("已启动 {}", guard::gui_agent_name(id))),
                Err(e) => sh.log(e),
            }
        } else {
            sh.log("未启动：请先开启守护、勾选该应用并通过出口检测");
        }
    }
    *sh.last.lock().unwrap() = Some(out);
    sh.last_duration_ms.store(duration_ms, Ordering::SeqCst);
    sh.last_checked_at.store(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        Ordering::SeqCst,
    );
    drop(current);
    sh.busy.store(false, Ordering::SeqCst);
    sh.emit_status();
}
pub fn spawn_check(sh: &Arc<Shared>, purpose: Purpose) -> Result<(), String> {
    sh.request_check(purpose)
}
pub fn spawn_monitor(sh: Arc<Shared>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut next = Instant::now();
        loop {
            let purpose = {
                let mut pending = sh.pending.lock().unwrap();
                loop {
                    if sh.stop.load(Ordering::SeqCst) {
                        return;
                    }
                    if let Some(purpose) = pending.pop_front() {
                        break purpose;
                    }
                    let now = Instant::now();
                    if now >= next {
                        break Purpose::Monitor;
                    }
                    pending = sh.wake.wait_timeout(pending, next - now).unwrap().0;
                }
            };
            run_check(&sh, purpose);
            next = Instant::now()
                + Duration::from_secs(sh.cfg.lock().unwrap().check_interval_secs.clamp(1, 60));
        }
    })
}

/// Diagnostics are independent of the guard worker and never invoke guard::trip.
pub fn run_risk_check(sh: &Arc<Shared>, deep: bool) -> Result<RiskReport, String> {
    if sh.risk_busy.swap(true, Ordering::SeqCst) {
        return Err("风险检查正在进行，请稍后重试".into());
    }
    sh.emit_status();
    let (cfg, revision) = {
        let cfg = sh.cfg.lock().unwrap();
        (cfg.clone(), sh.revision.load(Ordering::SeqCst))
    };
    let mut report = risks::inspect(&cfg, deep);
    let current = sh.cfg.lock().unwrap();
    let valid = !sh.stop.load(Ordering::SeqCst) && revision == sh.revision.load(Ordering::SeqCst);
    if valid {
        let mut stored = sh.risks.lock().unwrap();
        if !deep {
            if let Some(previous) = stored.as_ref() {
                report.ipv6_probe = previous.ipv6_probe.clone();
                report.dns_probe = previous.dns_probe.clone();
                report.deep_checked_at = previous.deep_checked_at;
            }
        }
        *stored = Some(report.clone());
    }
    drop(current);
    sh.risk_busy.store(false, Ordering::SeqCst);
    sh.emit_status();
    if valid {
        Ok(report)
    } else {
        Err("设置已改变或守护器退出，请重新检测".into())
    }
}
pub fn spawn_risk_monitor(sh: Arc<Shared>) {
    std::thread::spawn(move || {
        while !sh.stop.load(Ordering::SeqCst) {
            let _ = run_risk_check(&sh, false);
            std::thread::park_timeout(Duration::from_secs(60));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn armed() -> Config {
        Config {
            armed: true,
            first_run: false,
            allowed_ips: vec!["203.0.113.1".into()],
            ..Config::default()
        }
    }
    #[test]
    fn mismatched_ip_and_missing_proxy_trip_immediately() {
        assert!(should_trip(&armed(), false, true, 1));
    }
    #[test]
    fn unavailable_probes_use_threshold_and_success_resets() {
        assert!(!should_trip(&armed(), false, false, 1));
        assert!(should_trip(&armed(), false, false, 2));
        assert!(!should_trip(&armed(), true, true, 5));
    }
    #[test]
    fn initial_and_disarmed_states_never_kill() {
        assert!(!should_trip(&Config::default(), false, true, 100));
        let mut c = armed();
        c.armed = false;
        assert!(!should_trip(&c, false, true, 100));
        c.armed = true;
        c.first_run = true;
        assert!(!should_trip(&c, false, true, 100));
    }
    #[test]
    fn arming_requires_valid_configuration() {
        let mut c = Config {
            armed: true,
            ..Config::default()
        };
        assert!(validate_config(&c).is_err());
        c.allowed_ips = vec!["203.0.113.1".into()];
        assert!(validate_config(&c).is_ok());
        c.allowed_ips = vec!["not-an-ip".into()];
        assert!(validate_config(&c).is_err());
    }
    #[test]
    fn queued_manual_requests_survive_busy_and_wake_worker() {
        let sh = Arc::new(Shared::new(Config {
            check_interval_secs: 60,
            ..Config::default()
        }));
        sh.busy.store(true, Ordering::SeqCst);
        sh.request_check(Purpose::Manual).unwrap();
        sh.request_check(Purpose::Manual).unwrap();
        assert_eq!(sh.pending.lock().unwrap().len(), 1);
        sh.busy.store(false, Ordering::SeqCst);
        let worker = spawn_monitor(sh.clone());
        let wait_until = |count| {
            let deadline = Instant::now() + Duration::from_secs(3);
            while *sh.failures.lock().unwrap() < count && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            *sh.failures.lock().unwrap() >= count
        };
        let first = wait_until(1);
        sh.request_check(Purpose::Manual).unwrap();
        let second = wait_until(2);
        sh.stop.store(true, Ordering::SeqCst);
        sh.wake_monitor();
        worker.join().unwrap();
        assert!(
            first && second,
            "manual requests must run without waiting 60 seconds"
        );
        assert!(sh.snapshot().last_checked_at > 0);
    }
}
