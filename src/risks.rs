//! Advisory diagnostics only. These results never enter the termination decision.
use crate::config::Config;
use serde::Serialize;
use std::{
    collections::BTreeSet,
    io::Read,
    net::{IpAddr, Ipv6Addr, SocketAddr, UdpSocket},
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    pub state: &'static str,
    pub text: String,
}
impl Finding {
    fn new(state: &'static str, text: impl Into<String>) -> Self {
        Self {
            state,
            text: text.into(),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskReport {
    pub ipv6: Finding,
    pub dns: Finding,
    pub ipv6_probe: Option<Finding>,
    pub dns_probe: Option<Finding>,
    pub checked_at: u64,
    pub deep_checked_at: Option<u64>,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
// Bound subprocess lifetime and output; never inherit HTTP proxy environment variables.
fn command(exe: &str, args: &[&str], timeout: u64) -> Result<String, String> {
    let mut child = Command::new(exe)
        .args(args)
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "命令不可用".to_string())?;
    let deadline = Instant::now() + Duration::from_secs(timeout);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut text = String::new();
                if let Some(stdout) = child.stdout.take() {
                    stdout
                        .take(64 * 1024)
                        .read_to_string(&mut text)
                        .map_err(|_| "结果无法读取".to_string())?;
                }
                if let Some(stderr) = child.stderr.take() {
                    stderr
                        .take(8 * 1024)
                        .read_to_string(&mut text)
                        .map_err(|_| "结果无法读取".to_string())?;
                }
                return if status.success() {
                    Ok(text)
                } else {
                    Err("请求未完成".into())
                };
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("请求超时或失败".into());
            }
        }
    }
}
fn public_v6(ip: Ipv6Addr) -> bool {
    // Restrict to globally routed unicast; exclude documentation IPv6 addresses.
    let s = ip.segments();
    s[0] & 0xe000 == 0x2000 && !(s[0] == 0x2001 && s[1] == 0x0db8)
}
fn v6_count(text: &str) -> usize {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            if words.next()? != "inet6" {
                return None;
            }
            words.next()?.split('%').next()?.parse::<Ipv6Addr>().ok()
        })
        .filter(|ip| public_v6(*ip))
        .count()
}
fn dns_servers(text: &str) -> Vec<IpAddr> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.trim().split_once(" : ")?;
            if !key.starts_with("nameserver[") {
                return None;
            }
            value.trim().parse().ok()
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
fn route_interface(text: &str) -> Option<String> {
    // macOS route can return exit code 0 even when the destination is unreachable.
    if text.contains("not in table") || text.contains("Network is unreachable") {
        return None;
    }
    text.lines().find_map(|line| {
        line.trim()
            .strip_prefix("interface:")
            .map(|s| s.trim().into())
    })
}
fn route(ip: IpAddr) -> Result<Option<String>, String> {
    let address = ip.to_string();
    let family = if ip.is_ipv6() { "-inet6" } else { "-inet" };
    let text = command("/sbin/route", &["-n", "get", family, &address], 3)?;
    if text.contains("not in table") || text.contains("Network is unreachable") {
        return Ok(None);
    }
    route_interface(&text)
        .map(Some)
        .ok_or_else(|| "未能解析路由结果".into())
}
fn interface(ip: IpAddr) -> Option<String> {
    route(ip).ok().flatten()
}
// Read only the simple Boolean settings in the generated YAML. Unsupported syntax is unknown.
fn yaml_bool(text: &str, section: Option<&str>, key: &str) -> Option<bool> {
    let mut inside = section.is_none();
    let mut child_indent = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let top = !line.starts_with(char::is_whitespace);
        if let Some(section) = section {
            if top {
                inside = trimmed == format!("{section}:");
                child_indent = None;
                continue;
            }
        } else if !top {
            continue;
        }
        if inside {
            if section.is_some() {
                let indent = line.len() - line.trim_start().len();
                let direct = *child_indent.get_or_insert(indent);
                if indent != direct {
                    continue;
                }
            }
            let Some((k, value)) = trimmed.split_once(':') else {
                continue;
            };
            if k != key {
                continue;
            }
            return match value.split('#').next()?.trim() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            };
        }
    }
    None
}
fn saved_clash() -> Option<String> {
    let home = std::env::var_os("HOME")?;
    std::fs::read_to_string(PathBuf::from(home).join(
        "Library/Application Support/io.github.clash-verge-rev.clash-verge-rev/clash-verge.yaml",
    ))
    .ok()
}
fn ipv6_finding(count: usize, route: Option<&str>) -> Finding {
    match (count, route) {
        (0, None) => Finding::new(
            "not_observed",
            "本次未发现公网 IPv6 地址或目标 IPv6 路由；换网络后需重测",
        ),
        (_, Some(name)) if name.starts_with("utun") => {
            Finding::new("unknown", "存在隧道 IPv6 路由；尚未确认实际出口")
        }
        (_, Some(_)) => Finding::new(
            "risk",
            "公网 IPv6 目标走物理网卡路由；可能绕过代理，需深度检测",
        ),
        (_, None) => Finding::new("unknown", "发现公网 IPv6 地址，尚未确认可用出口"),
    }
}
pub fn inspect(cfg: &Config, deep: bool) -> RiskReport {
    let mut report = RiskReport {
        ipv6: Finding::new("unknown", "无法读取网卡或路由，未完成核验"),
        dns: Finding::new("unknown", "无法读取系统 DNS，未完成核验"),
        ipv6_probe: None,
        dns_probe: None,
        checked_at: now(),
        deep_checked_at: None,
    };
    if let Ok(text) = command("/sbin/ifconfig", &[], 3) {
        let target: IpAddr = "2606:4700:4700::1111".parse().unwrap();
        if let Ok(route) = route(target) {
            report.ipv6 = ipv6_finding(v6_count(&text), route.as_deref());
        }
    }
    let servers = command("/usr/sbin/scutil", &["--dns"], 3)
        .ok()
        .map(|s| dns_servers(&s));
    if let Some(servers) = &servers {
        let external = servers.iter().filter(|s| !s.is_loopback()).count();
        report.dns = if servers.is_empty() {
            Finding::new("unknown", "没有可用的系统 DNS 信息；不能确认解析路径")
        } else if external > 0 {
            Finding::new("unknown", format!("系统包含 {external} 个非回环 DNS 地址；这本身不表示泄漏，请查看路径测试是否经隧道路由"))
        } else {
            Finding::new("unknown", "系统 DNS 指向本机；还需核验本机解析器的上游路径")
        };
    }
    if let Some(text) = saved_clash() {
        let top = yaml_bool(&text, None, "ipv6");
        let dns6 = yaml_bool(&text, Some("dns"), "ipv6");
        let enabled = yaml_bool(&text, Some("dns"), "enable");
        report.ipv6.text.push_str(&format!(
            "。Clash 保存配置：IPv6 {}、DNS IPv6 {}（非内核实时状态）",
            setting(top),
            setting(dns6)
        ));
        if enabled == Some(false) {
            report.dns = Finding::new(
                "risk",
                "Clash 保存配置未启用 DNS 模块；需核验系统解析是否绕行",
            );
        }
    }
    if deep {
        report.ipv6_probe = Some(probe_v6(cfg));
        report.dns_probe = Some(probe_dns(servers.as_deref().unwrap_or(&[])));
        report.deep_checked_at = Some(now());
    }
    report.checked_at = now();
    report
}
fn setting(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "开启",
        Some(false) => "关闭",
        None => "未知",
    }
}
fn probe_v6(cfg: &Config) -> Finding {
    // Resolve AAAA over the existing proxy so suppressed system AAAA replies cannot hide a path.
    if !matches!(cfg.proxy_host.as_str(), "127.0.0.1" | "localhost" | "::1") || cfg.proxy_port == 0
    {
        return Finding::new("unknown", "本机代理配置无效，未执行 IPv6 出口测试");
    }
    let host = if cfg.proxy_host == "::1" {
        "[::1]"
    } else {
        &cfg.proxy_host
    };
    let proxy = format!("http://{host}:{}", cfg.proxy_port);
    let lookup = command(
        "/usr/bin/curl",
        &[
            "-q",
            "-sS",
            "--proxy",
            &proxy,
            "--noproxy",
            "",
            "--connect-timeout",
            "3",
            "--max-time",
            "6",
            "-H",
            "accept: application/dns-json",
            "https://cloudflare-dns.com/dns-query?name=api6.ipify.org&type=AAAA",
        ],
        7,
    );
    let address = lookup
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| {
            v.get("Answer")?.as_array()?.iter().find_map(|a| {
                if a.get("type")?.as_u64()? != 28 {
                    return None;
                }
                a.get("data")?.as_str()?.parse::<Ipv6Addr>().ok()
            })
        });
    let Some(address) = address else {
        return Finding::new("unknown", "独立 AAAA 查询失败，不能核验 IPv6 出口");
    };
    let resolve = format!("api6.ipify.org:443:[{address}]");
    let result = command(
        "/usr/bin/curl",
        &[
            "-q",
            "-sS",
            "--noproxy",
            "*",
            "-6",
            "--resolve",
            &resolve,
            "--connect-timeout",
            "3",
            "--max-time",
            "6",
            "https://api6.ipify.org?format=json",
        ],
        7,
    );
    match result {
        Ok(body) => match serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("ip")?.as_str()?.parse::<IpAddr>().ok())
        {
            Some(ip @ IpAddr::V6(_)) if crate::checks::ip_allowed(cfg, &ip.to_string()) => {
                Finding::new(
                    "not_observed",
                    "本次 IPv6 出口命中白名单；其他应用连接尚未逐条核验",
                )
            }
            Some(IpAddr::V6(_)) => {
                Finding::new("risk", "已检测到白名单之外的 IPv6 出口。仅提示，未关闭应用")
            }
            _ => Finding::new("unknown", "服务未返回有效 IPv6 地址，不能判断出口"),
        },
        Err(_) => Finding::new(
            "not_observed",
            "本次指定 IPv6 目标连接未成功；单次失败不能证明所有 IPv6 流量均被阻断",
        ),
    }
}
fn dns_query(id: u16) -> Vec<u8> {
    let mut q = vec![];
    for value in [id, 0x0100, 1, 0, 0, 0] {
        q.extend_from_slice(&value.to_be_bytes());
    }
    // Fresh reserved .invalid name: no private hostname or credentials are transmitted.
    let label = format!("cg-{}-{id}", now());
    q.push(label.len() as u8);
    q.extend_from_slice(label.as_bytes());
    q.push(7);
    q.extend_from_slice(b"invalid");
    q.push(0);
    q.extend_from_slice(&[0, 1, 0, 1]);
    q
}
fn valid_reply(bytes: &[u8], id: u16) -> bool {
    bytes.len() >= 12 && bytes[..2] == id.to_be_bytes() && bytes[2] & 0x80 != 0
}
fn probe_dns(servers: &[IpAddr]) -> Finding {
    if servers.is_empty() {
        return Finding::new("unknown", "没有系统 DNS 地址，无法进行路径检查");
    }
    let mut answered = 0;
    let mut physical = 0;
    let mut tunneled = 0;
    for (index, ip) in servers.iter().take(3).enumerate() {
        let bind = if ip.is_ipv6() { "[::]:0" } else { "0.0.0.0:0" };
        let Ok(socket) = UdpSocket::bind(bind) else {
            continue;
        };
        let _ = socket.set_read_timeout(Some(Duration::from_millis(700)));
        let _ = socket.set_write_timeout(Some(Duration::from_millis(700)));
        if socket.connect(SocketAddr::new(*ip, 53)).is_err() {
            continue;
        }
        let id = (now() as u16).wrapping_add(index as u16);
        if socket.send(&dns_query(id)).is_err() {
            continue;
        }
        let mut buffer = [0; 4096];
        if let Ok(n) = socket.recv(&mut buffer) {
            if valid_reply(&buffer[..n], id) {
                answered += 1;
                if !ip.is_loopback() {
                    match interface(*ip) {
                        Some(name) if name.starts_with("utun") => tunneled += 1,
                        Some(_) => physical += 1,
                        None => {}
                    }
                }
            }
        }
    }
    dns_probe_summary(answered, physical, tunneled)
}
fn dns_probe_summary(answered: usize, physical: usize, tunneled: usize) -> Finding {
    if physical > 0 {
        Finding::new("risk", format!("{physical} 个按非隧道路由的系统 DNS 对测试查询应答；存在绕行风险，但未核验实际数据包路径和递归解析器公网出口"))
    } else if tunneled > 0 {
        Finding::new("unknown", format!("测试收到 {answered} 个 DNS 应答，其中 {tunneled} 个目标按隧道路由；未发现非隧道路由应答证据。路由不等于抓包证明，上游解析器公网出口尚未核验"))
    } else {
        Finding::new("unknown", format!("测试收到 {answered} 个 DNS 应答；未能确认隧道或非隧道应答路径。缓存、拦截和上游转发均可能影响结果，不能据此排除 DNS 泄漏"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tunnel_dns_evidence_does_not_claim_complete_protection() {
        let tunnel = dns_probe_summary(1, 0, 1);
        assert_eq!(tunnel.state, "unknown");
        assert!(tunnel.text.contains("按隧道路由"));
        assert!(tunnel.text.contains("尚未核验"));
        assert_eq!(dns_probe_summary(2, 1, 1).state, "risk");
        assert_eq!(dns_probe_summary(0, 0, 0).state, "unknown");
    }
    #[test]
    fn local_ipv6_is_not_public_risk() {
        assert_eq!(v6_count(" inet6 ::1 prefixlen 128\n inet6 fe80::1%en0 prefixlen 64\n inet6 fd00::1 prefixlen 64\n inet6 2001:db8::1 prefixlen 64"), 0);
        assert_eq!(v6_count(" inet6 2606:4700::1 prefixlen 64"), 1);
    }
    #[test]
    fn physical_route_warns_but_tunnel_route_is_unverified() {
        assert_eq!(ipv6_finding(1, Some("en0")).state, "risk");
        assert_eq!(ipv6_finding(1, Some("utun2")).state, "unknown");
        assert_eq!(ipv6_finding(0, None).state, "not_observed");
        assert_eq!(
            route_interface("route: writing to routing socket: not in table"),
            None
        );
    }
    #[test]
    fn yaml_sections_do_not_confuse_dns_and_kernel_switches() {
        let yaml = "ipv6: false\ndns:\n  enable: true\n  ipv6: true\ntun:\n  enable: false\n";
        assert_eq!(yaml_bool(yaml, None, "ipv6"), Some(false));
        assert_eq!(yaml_bool(yaml, Some("dns"), "ipv6"), Some(true));
        assert_eq!(yaml_bool(yaml, Some("dns"), "enable"), Some(true));
        assert_eq!(yaml_bool("dns: *alias", Some("dns"), "enable"), None);
        assert_eq!(
            yaml_bool(
                "dns:\n  nested:\n    ipv6: true\n  enable: false\n",
                Some("dns"),
                "ipv6"
            ),
            None
        );
    }
    #[test]
    fn dns_reply_requires_matching_transaction_and_response_flag() {
        let query = dns_query(42);
        assert!(!valid_reply(&query, 42));
        let mut reply = query;
        reply[2] |= 0x80;
        assert!(valid_reply(&reply, 42));
        assert!(!valid_reply(&reply, 43));
        assert!(!valid_reply(&reply[..4], 42));
        assert_eq!(dns_servers(" nameserver[0] : 127.0.0.1\n nameserver[1] : 192.0.2.1\n nameserver[2] : 127.0.0.1").len(), 2);
    }
}
