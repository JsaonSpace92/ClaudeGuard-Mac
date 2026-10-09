//! Read-only dashboard probes. They describe this process, never target-app flows.
use crate::config::Config;
use serde_json::{json, Value};
use std::{
    io::Read,
    net::IpAddr,
    time::{Duration, Instant},
};

fn exit_record(value: &Value, ipv4_only: bool) -> Option<Value> {
    let ip = value.get("ip")?.as_str()?.parse::<IpAddr>().ok()?;
    if ipv4_only && !ip.is_ipv4() {
        return None;
    }
    let field = |name| {
        value
            .get(name)
            .and_then(Value::as_str)
            .unwrap_or("")
            .chars()
            .take(128)
            .collect::<String>()
    };
    Some(
        json!({"state":"observed","ip":ip.to_string(),"country":field("country"),"region":field("region"),"city":field("city"),"timezone":field("timezone"),"org":field("org")}),
    )
}
fn exit(agent: &ureq::Agent, ipv4_only: bool) -> Value {
    for url in [
        "https://ipinfo.io/json",
        "https://api4.ipify.org?format=json",
    ] {
        let Ok(response) = agent.get(url).call() else {
            continue;
        };
        let mut body = String::new();
        if response
            .into_reader()
            .take(4097)
            .read_to_string(&mut body)
            .is_err()
            || body.len() > 4096
        {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(&body) {
            if let Some(record) = exit_record(&value, ipv4_only) {
                return record;
            }
        }
    }
    json!({"state":"unknown","text":"未获得有效出口；服务可能不可达"})
}
fn connectivity_status(status: u16) -> (&'static str, &'static str) {
    match status {
        200..=299 | 400 | 401 | 404 | 405 => ("observed", "检测端点可达"),
        403 => ("unknown", "端点返回 403，访问受限"),
        429 => ("unknown", "端点返回 429，暂时限流"),
        _ => ("unknown", "端点未返回预期状态"),
    }
}

/// Fresh proxy-only discovery works even before the user has a whitelist.
/// Never fall back to the system route when the local proxy is unavailable.
pub fn proxy_exit(cfg: &Config) -> Value {
    if !matches!(cfg.proxy_host.as_str(), "127.0.0.1" | "localhost" | "::1") || cfg.proxy_port == 0
    {
        return json!({"state":"unknown","text":"仅支持有效的本机 HTTP / Mixed 代理入口"});
    }
    let host = if cfg.proxy_host == "::1" {
        "[::1]"
    } else {
        &cfg.proxy_host
    };
    let Ok(proxy) = ureq::Proxy::new(format!("http://{host}:{}", cfg.proxy_port)) else {
        return json!({"state":"unknown","text":"代理入口无效"});
    };
    let agent = ureq::AgentBuilder::new()
        .try_proxy_from_env(false)
        .proxy(proxy)
        .redirects(0)
        .timeout(Duration::from_secs(cfg.probe_timeout_secs.clamp(1, 10)))
        .build();
    exit(&agent, false)
}

fn feed_result(text: &str, target: IpAddr) -> Result<Value, &'static str> {
    if !target.is_ipv4() {
        return Err("当前公开名单只覆盖 IPv4，IPv6 信誉尚未核验");
    }
    if !text.starts_with("# IPsum Threat Intelligence Feed") {
        return Err("公开名单格式无效");
    }
    let mut hits = 0u16;
    let mut entries = 0;
    let mut updated = String::new();
    for line in text.lines() {
        if let Some(date) = line.strip_prefix("# Last update:") {
            updated = date.trim().chars().take(100).collect();
        }
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() != 2 {
            return Err("公开名单记录无效");
        }
        let ip = fields[0]
            .parse::<std::net::Ipv4Addr>()
            .map_err(|_| "公开名单地址无效")?;
        let count = fields[1]
            .parse::<u16>()
            .ok()
            .filter(|v| *v > 0 && *v <= 1024)
            .ok_or("公开名单计数无效")?;
        entries += 1;
        if IpAddr::V4(ip) == target {
            hits = hits.max(count);
        }
    }
    if entries == 0 || updated.is_empty() {
        return Err("公开名单为空或缺少日期，未完成核验");
    }
    Ok(
        json!({"state":"observed","source":"IPsum","sourceUrl":"https://github.com/stamparm/ipsum","feedUpdated":updated,"entries":entries,"listedBy":hits,"ip":target.to_string(),"text":if hits>0 {format!("公开名单命中 {hits} 个来源，需复核")} else {"本次公开名单未命中".into()},"scope":"本机比对公开名单；命中可能有误报，未命中不代表账号安全，也不是 Claude 风控评分"}),
    )
}
fn reputation(agent: &ureq::Agent, ip: IpAddr) -> Value {
    let run = || -> Result<Value, String> {
        let response = agent
            .get("https://raw.githubusercontent.com/stamparm/ipsum/master/ipsum.txt")
            .call()
            .map_err(|_| "公开信誉名单暂不可达")?;
        let mut text = String::new();
        response
            .into_reader()
            .take(8 * 1024 * 1024 + 1)
            .read_to_string(&mut text)
            .map_err(|_| "公开名单无法读取")?;
        if text.len() > 8 * 1024 * 1024 {
            return Err("公开名单超过大小限制".into());
        }
        feed_result(&text, ip).map_err(str::to_string)
    };
    run().unwrap_or_else(|error| json!({"state":"unknown","text":error,"source":"IPsum"}))
}
pub fn inspect(cfg: &Config) -> Value {
    let direct = ureq::AgentBuilder::new()
        .try_proxy_from_env(false)
        .redirects(0)
        .timeout(Duration::from_secs(4))
        .build();
    let host = if cfg.proxy_host == "::1" {
        "[::1]"
    } else {
        &cfg.proxy_host
    };
    let proxy = ureq::Proxy::new(format!("http://{host}:{}", cfg.proxy_port))
        .ok()
        .map(|p| {
            ureq::AgentBuilder::new()
                .proxy(p)
                .redirects(0)
                .timeout(Duration::from_secs(4))
                .build()
        });
    let (system, proxy_exit, claude) = std::thread::scope(|scope| {
        let system = scope.spawn(|| exit(&direct, true));
        let proxy_exit = scope.spawn(|| {
            proxy
                .as_ref()
                .map(|agent| exit(agent, false))
                .unwrap_or_else(|| json!({"state":"unknown","text":"代理入口无效"}))
        });
        let claude = scope.spawn(|| {
            let Some(agent) = proxy.as_ref() else { return json!({"state":"unknown","text":"代理入口无效"}); };
            let start = Instant::now();
            let status = match agent.get("https://api.anthropic.com/").call() {
                Ok(response) => Some(response.status()),
                Err(ureq::Error::Status(status, _)) => Some(status),
                Err(_) => None,
            };
            let (state, text) = status.map(connectivity_status).unwrap_or(("unknown", "未能连接检测端点"));
            json!({"state":state,"text":text,"httpStatus":status,"latencyMs":start.elapsed().as_millis(),"scope":"ClaudeGuard 经本机代理访问 api.anthropic.com；不代表 Claude App 登录或每条连接"})
        });
        (
            system.join().unwrap_or(Value::Null),
            proxy_exit.join().unwrap_or(Value::Null),
            claude.join().unwrap_or(Value::Null),
        )
    });
    let reputation = proxy_exit
        .get("ip")
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<IpAddr>().ok())
        .zip(proxy.as_ref())
        .map(|(ip, agent)| reputation(agent, ip))
        .unwrap_or_else(|| json!({"state":"unknown","text":"需先取得有效代理出口"}));
    json!({"system":system,"proxy":proxy_exit,"claude":claude,"reputation":reputation,"scope":"系统出口使用系统路由，可能经过 TUN；代理出口属于检测请求，不等于 Claude 实际连接"})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_rejects_nonlocal_proxy_without_using_direct_route() {
        let cfg = Config {
            proxy_host: "example.com".into(),
            ..Config::default()
        };
        assert_eq!(proxy_exit(&cfg)["state"], "unknown");
        let cfg = Config {
            proxy_port: 0,
            ..Config::default()
        };
        assert_eq!(proxy_exit(&cfg)["state"], "unknown");
    }
    #[test]
    fn validates_exit_identity_and_limits_metadata() {
        assert!(exit_record(&json!({"ip":"203.0.113.1.evil"}), false).is_none());
        assert!(exit_record(&json!({"ip":"2001:db8::1"}), true).is_none());
        let record =
            exit_record(&json!({"ip":"203.0.113.7","city":"x".repeat(200)}), true).unwrap();
        assert_eq!(record["city"].as_str().unwrap().len(), 128);
    }
    #[test]
    fn denial_and_rate_limit_do_not_pass_connectivity() {
        assert_eq!(connectivity_status(401).0, "observed");
        assert_eq!(connectivity_status(403).0, "unknown");
        assert_eq!(connectivity_status(429).0, "unknown");
        assert_eq!(connectivity_status(500).0, "unknown");
    }

    #[test]
    fn reputation_feed_is_exact_and_invalid_data_is_never_clear() {
        let target = "203.0.113.7".parse().unwrap();
        let header = "# IPsum Threat Intelligence Feed\n# Last update: fixture date\n";
        assert_eq!(
            feed_result(
                &format!("{header}203.0.113.70\t4\n203.0.113.7\t2\n"),
                target
            )
            .unwrap()["listedBy"],
            2
        );
        assert_eq!(
            feed_result(&format!("{header}203.0.113.70\t4\n"), target).unwrap()["listedBy"],
            0
        );
        assert!(feed_result("<html>not a feed</html>", target).is_err());
        assert!(feed_result(&format!("{header}bad-ip\t4\n"), target).is_err());
        assert!(feed_result(header, target).is_err());
        assert!(feed_result(
            &format!("{header}203.0.113.7\t4\n"),
            "2001:db8::1".parse().unwrap()
        )
        .is_err());
    }
}
