//! macOS: explicitly query the existing HTTP/mixed proxy. No system-proxy requirement.
use crate::config::Config;
use serde::Serialize;
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Pass,
    Fail,
    Skip,
}
#[derive(Debug, Clone, Serialize)]
pub struct StepResult {
    pub state: StepState,
    pub text: String,
}
impl StepResult {
    fn pass(text: impl Into<String>) -> Self {
        Self {
            state: StepState::Pass,
            text: text.into(),
        }
    }
    fn fail(text: impl Into<String>) -> Self {
        Self {
            state: StepState::Fail,
            text: text.into(),
        }
    }
    fn skip() -> Self {
        Self {
            state: StepState::Skip,
            text: "未检测".into(),
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct CheckOutcome {
    pub proxy: StepResult,
    pub port: StepResult,
    pub egress: StepResult,
    pub passed: bool,
    pub reason: String,
    pub egress_ip: Option<String>,
    pub egress_desc: Option<String>,
    pub egress_country: Option<String>,
    /// Known wrong public IP trips immediately; unavailable probes use a failure threshold.
    pub confirmed_mismatch: bool,
}

pub fn ip_allowed(cfg: &Config, ip: &str) -> bool {
    let Ok(ip) = ip.parse::<IpAddr>() else {
        return false;
    };
    cfg.allowed_ips
        .iter()
        .filter_map(|s| s.parse::<IpAddr>().ok())
        .any(|a| a == ip)
}

fn probe(agent: &ureq::Agent, url: &str) -> Result<String, String> {
    let response = agent
        .get(url)
        .set("Cache-Control", "no-cache")
        .call()
        .map_err(|e| e.to_string())?;
    let mut body = String::new();
    use std::io::Read;
    response
        .into_reader()
        .take(4096)
        .read_to_string(&mut body)
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let ip = v.get("ip").and_then(|v| v.as_str()).ok_or("IP 字段缺失")?;
    ip.parse::<IpAddr>()
        .map(|ip| ip.to_string())
        .map_err(|_| "IP 格式无效".into())
}

/// Full check has a finite network deadline. Probe connections are fresh each cycle.
pub fn run_checks(cfg: &Config, mut log: impl FnMut(String)) -> CheckOutcome {
    let mut out = CheckOutcome {
        proxy: StepResult::skip(),
        port: StepResult::skip(),
        egress: StepResult::skip(),
        passed: false,
        reason: String::new(),
        egress_ip: None,
        egress_desc: None,
        egress_country: None,
        confirmed_mismatch: false,
    };
    if cfg.allowed_ips.is_empty() {
        out.proxy = StepResult::fail("请先填写允许的静态出口 IP");
        out.reason = "尚未配置出口 IP".into();
        return out;
    }
    if cfg.proxy_host != "127.0.0.1" && cfg.proxy_host != "localhost" && cfg.proxy_host != "::1" {
        out.proxy = StepResult::fail("第一版仅支持本机 Clash 代理入口");
        out.reason = out.proxy.text.clone();
        return out;
    }
    out.proxy = StepResult::pass(format!(
        "显式代理 {}:{}；支持 TUN，无需系统代理",
        cfg.proxy_host, cfg.proxy_port
    ));
    let timeout = Duration::from_secs(cfg.probe_timeout_secs.clamp(1, 10));
    let address = match (cfg.proxy_host.as_str(), cfg.proxy_port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
    {
        Some(a) => a,
        None => {
            out.reason = "代理地址无效".into();
            out.port = StepResult::fail(&out.reason);
            return out;
        }
    };
    if let Err(e) = TcpStream::connect_timeout(&address, Duration::from_millis(500)) {
        out.reason = "Clash 代理入口不可用".into();
        out.port = StepResult::fail(format!("{}：{e}", out.reason));
        return out;
    }
    out.port = StepResult::pass("本机入口可以连接");
    let host = if cfg.proxy_host == "::1" {
        "[::1]"
    } else {
        &cfg.proxy_host
    };
    let proxy = match ureq::Proxy::new(format!("http://{host}:{}", cfg.proxy_port)) {
        Ok(p) => p,
        Err(e) => {
            out.reason = e.to_string();
            return out;
        }
    };
    let agent = ureq::AgentBuilder::new()
        .proxy(proxy)
        .timeout(timeout)
        .timeout_connect(timeout)
        .timeout_read(timeout)
        .timeout_write(timeout)
        .build();
    let (a, b) = std::thread::scope(|s| {
        let a = s.spawn(|| probe(&agent, "https://api.ipify.org?format=json"));
        let b = s.spawn(|| probe(&agent, "https://ipinfo.io/json"));
        (
            a.join().unwrap_or_else(|_| Err("探测线程失败".into())),
            b.join().unwrap_or_else(|_| Err("探测线程失败".into())),
        )
    });
    let ips: Vec<String> = [&a, &b]
        .into_iter()
        .filter_map(|r| r.as_ref().ok().cloned())
        .collect();
    if ips.is_empty() {
        out.reason = "无法确认出口 IP（两个探测服务均失败）".into();
        out.egress = StepResult::fail(&out.reason);
        log(format!("探测不可用: ipify={a:?}, ipinfo={b:?}"));
        return out;
    }
    out.egress_ip = ips.first().cloned();
    if let Some(wrong) = ips.iter().find(|ip| !ip_allowed(cfg, ip)) {
        out.egress_ip = Some(wrong.clone());
        out.confirmed_mismatch = true;
        out.reason = format!("出口 {wrong} 不在静态 IP 白名单中");
        out.egress = StepResult::fail(&out.reason);
    } else {
        out.passed = true;
        out.egress = StepResult::pass(format!("{} · {}/2 个探测成功", ips[0], ips.len()));
        out.egress_desc = Some("检测请求经过 Clash 显式代理入口".into());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_ip_only_country_never_bypasses() {
        let cfg = Config {
            allowed_ips: vec!["203.0.113.1".into()],
            egress_region: "US".into(),
            ..Config::default()
        };
        assert!(ip_allowed(&cfg, "203.0.113.1"));
        assert!(!ip_allowed(&cfg, "203.0.113.2"));
        assert!(!ip_allowed(&cfg, "203.0.113.1.evil"));
    }
    #[test]
    fn empty_configuration_is_non_passing() {
        let result = run_checks(&Config::default(), |_| {});
        assert!(!result.passed);
        assert!(!result.confirmed_mismatch);
    }
    #[test]
    fn closed_proxy_is_failure_without_egress() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let cfg = Config {
            proxy_port: port,
            allowed_ips: vec!["203.0.113.1".into()],
            ..Config::default()
        };
        let result = run_checks(&cfg, |_| {});
        assert!(!result.passed);
        assert_eq!(result.port.state, StepState::Fail);
    }
}
