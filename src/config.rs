use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct Config {
    /// Explicit opt-in: a fresh install never terminates applications.
    pub armed: bool,
    pub failure_threshold: u32,
    pub probe_timeout_secs: u64,
    pub proxy_host: String,
    pub proxy_port: u16,
    /// 约定出口 IP 列表（v2.6 起）：命中任意一个即视为出口正确。
    #[serde(default)]
    pub allowed_ips: Vec<String>,
    /// 上游兼容字段；macOS 版不使用地区放行。
    #[serde(default)]
    pub egress_region: String,
    /// 兼容字段：v1~v2.5 的单值 required_ip。仅用于读取迁移，不再写出。
    #[serde(default, skip_serializing)]
    pub required_ip: Option<String>,
    /// 守护对象：claude / chatgpt / antigravity；异常时只关闭应用包内进程。
    #[serde(default = "default_guarded_agents")]
    pub guarded_agents: Vec<String>,
    /// 上游兼容字段；macOS 版不处理独立 CLI。
    #[serde(default = "default_true")]
    pub cli_warn: bool,
    pub connect_target: String,
    /// 上游 Windows 配置兼容字段，macOS 版不使用。
    pub app_id: String,
    pub check_interval_secs: u64,
    pub kill_on_fail: bool,
    pub quarantine_on_fail: bool,
    /// 上游兼容字段；macOS 版不自动重启应用。
    pub launch_on_pass: bool,
    pub close_to_tray: bool,
    pub auto_start_with_system: bool,
    pub first_run: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            armed: false,
            failure_threshold: 2,
            probe_timeout_secs: 2,
            proxy_host: "127.0.0.1".into(),
            proxy_port: 7897,
            // 默认留空：出口 IP 属于用户隐私，由首次运行向导引导填写。
            // 空白名单无法开启守护；观察模式不会关闭应用。
            allowed_ips: Vec::new(),
            egress_region: String::new(),
            required_ip: None,
            guarded_agents: default_guarded_agents(),
            cli_warn: false,
            connect_target: "api.anthropic.com:443".into(),
            app_id: "Claude_pzs8sxrjxfjjc!Claude".into(),
            check_interval_secs: 2,
            kill_on_fail: true,
            quarantine_on_fail: false,
            launch_on_pass: false,
            close_to_tray: true,
            auto_start_with_system: false,
            first_run: true,
        }
    }
}

pub fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("CG_DATA_DIR") {
        return PathBuf::from(dir);
    }
    PathBuf::from(std::env::var_os("HOME").expect("HOME is required on macOS"))
        .join("Library/Application Support/ClaudeGuard Mac")
}

fn default_guarded_agents() -> Vec<String> {
    vec!["claude".into(), "chatgpt".into(), "antigravity".into()]
}

const fn default_true() -> bool {
    true
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

impl Config {
    pub fn load() -> Self {
        let p = config_path();
        if let Ok(text) = fs::read_to_string(&p) {
            match serde_json::from_str::<Config>(&text) {
                Ok(mut cfg) => {
                    let migrated = cfg.migrate();
                    if migrated {
                        // 老配置迁移成列表后立即落盘，之后 required_ip 不再出现在文件里
                        let _ = cfg.save();
                    }
                    return cfg;
                }
                // 解析失败不再静默：记日志后回退默认，便于排查被覆盖的配置
                Err(e) => {
                    crate::guard::log_line(&format!(
                        "config parse failed ({}), falling back to defaults",
                        e
                    ));
                }
            }
        }
        Self::default()
    }

    /// v2.6 迁移：旧单值 required_ip → allowed_ips 列表（去空白/去重）。
    /// 返回是否有改动（调用方决定是否落盘）。
    pub fn migrate(&mut self) -> bool {
        let mut changed = false;
        if let Some(ip) = self.required_ip.take() {
            let ip = ip.trim().to_string();
            if !ip.is_empty() && !self.allowed_ips.iter().any(|a| a == &ip) {
                self.allowed_ips.push(ip);
                changed = true;
            }
        }
        let clean: Vec<String> = self
            .allowed_ips
            .iter()
            .map(|s| {
                let ip = s.trim();
                ip.parse::<std::net::IpAddr>()
                    .map(|ip| ip.to_string())
                    .unwrap_or_else(|_| ip.to_string())
            })
            .filter(|s| !s.is_empty())
            .collect();
        let dedup: Vec<String> = {
            let mut seen = std::collections::HashSet::new();
            clean
                .into_iter()
                .filter(|s| seen.insert(s.clone()))
                .collect()
        };
        if dedup != self.allowed_ips {
            self.allowed_ips = dedup;
            changed = true;
        }
        self.egress_region = self.egress_region.trim().to_string();
        // v2.7：守护对象列表清洗——去空白/去重/剔除未知 id（registry 变更后老配置不残留死 id）。
        let known = crate::guard::gui_agent_ids();
        let raw = self.guarded_agents.clone();
        let mut cleaned: Vec<String> = Vec::new();
        for id in &raw {
            let id = id.trim().to_lowercase();
            if known.contains(&id.as_str()) && !cleaned.iter().any(|x| x == &id) {
                cleaned.push(id);
            }
        }
        // raw 全是未知 id 被清光 → 回填 claude（保守护，不静默裸奔）；
        // raw 本来就是空（用户显式清空）→ 尊重用户选择，什么都不杀。
        if cleaned.is_empty() && !raw.is_empty() {
            cleaned = default_guarded_agents();
        }
        let same = cleaned.len() == raw.len()
            && cleaned
                .iter()
                .zip(raw.iter())
                .all(|(a, b)| *a == b.trim().to_lowercase());
        if !same {
            self.guarded_agents = cleaned;
            changed = true;
        }
        changed
    }

    pub fn save(&self) -> std::io::Result<()> {
        let dir = config_dir();
        fs::create_dir_all(&dir)?;
        let text = serde_json::to_string_pretty(self).unwrap_or_default();
        // write via temp + rename for atomicity
        let tmp = dir.join("config.json.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, config_path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 行为锁: 配置 JSON 往返无损。
    #[test]
    fn json_round_trip() {
        let cfg = Config {
            armed: false,
            failure_threshold: 2,
            probe_timeout_secs: 2,
            proxy_host: "127.0.0.1".into(),
            proxy_port: 7890,
            allowed_ips: vec!["203.0.113.10".into(), "198.51.100.7".into()],
            egress_region: "US".into(),
            required_ip: None,
            guarded_agents: vec!["claude".into(), "chatgpt".into()],
            cli_warn: false,
            connect_target: "api.anthropic.com:443".into(),
            app_id: "Claude_pzs8sxrjxfjjc!Claude".into(),
            check_interval_secs: 15,
            kill_on_fail: true,
            quarantine_on_fail: true,
            launch_on_pass: true,
            close_to_tray: true,
            auto_start_with_system: false,
            first_run: false,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        // 兼容字段不落盘：旧 required_ip 不得再出现在文件里
        assert!(!json.contains("required_ip"));
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(back.allowed_ips, vec!["203.0.113.10", "198.51.100.7"]);
        assert_eq!(back.egress_region, "US");
        assert_eq!(back.guarded_agents, vec!["claude", "chatgpt"]);
        assert!(!back.cli_warn);
        assert_eq!(back.proxy_port, 7890);
        assert_eq!(back.check_interval_secs, 15);
    }

    /// 行为锁: 旧版本/缺字段的配置文件必须能读——serde(default) 兜底,
    /// 加新字段时不许破坏已装用户的配置。
    #[test]
    fn old_config_still_loads() {
        // v1.2 时代的最小配置(部分字段缺失，required_ip 还是单值)
        let legacy = r#"{
            "proxy_host": "127.0.0.1",
            "proxy_port": 7890,
            "required_ip": "203.0.113.10",
            "connect_target": "api.anthropic.com:443",
            "app_id": "Claude_pzs8sxrjxfjjc!Claude",
            "first_run": false
        }"#;
        let mut cfg: Config = serde_json::from_str(legacy).expect("旧配置必须可解析");
        assert_eq!(cfg.check_interval_secs, 2); // 缺省值兜底
        assert!(cfg.kill_on_fail); //              缺省值兜底
                                   // 迁移：单值进列表，兼容字段清空
        assert!(cfg.migrate());
        assert_eq!(cfg.allowed_ips, vec!["203.0.113.10"]);
        assert!(cfg.required_ip.is_none());
        // 再跑一次迁移必须幂等
        assert!(!cfg.migrate());
    }

    /// 行为锁: 列表清理——空白条目剔除、重复去重。
    #[test]
    fn migrate_cleans_ip_list() {
        let mut cfg = Config::default();
        cfg.allowed_ips = vec![
            " 1.2.3.4 ".into(),
            "".into(),
            "1.2.3.4".into(),
            "5.6.7.8".into(),
        ];
        assert!(cfg.migrate());
        assert_eq!(cfg.allowed_ips, vec!["1.2.3.4", "5.6.7.8"]);
    }

    #[test]
    fn whitelist_canonicalizes_ipv6_without_hiding_invalid_entries() {
        let mut cfg = Config {
            allowed_ips: vec![
                "2001:0DB8:0:0:0:0:0:1".into(),
                "2001:db8::1".into(),
                "bad-ip".into(),
            ],
            ..Config::default()
        };
        assert!(cfg.migrate());
        assert_eq!(cfg.allowed_ips, vec!["2001:db8::1", "bad-ip"]);
        assert!(crate::monitor::validate_config(&cfg).is_err());
    }

    /// 行为锁: v2.7 守护对象——默认只守 Claude；未知 id 清理但全垃圾时回填 claude；
    /// 用户显式清空（合法状态）不被回填。
    #[test]
    fn migrate_guarded_agents() {
        let mut cfg = Config::default();
        assert_eq!(cfg.guarded_agents, default_guarded_agents());
        assert!(!cfg.cli_warn);
        // 未知 id + 大小写/空白 → 清洗成合法集
        cfg.guarded_agents = vec![" Claude ".into(), "bogus".into(), "ChatGPT".into()];
        assert!(cfg.migrate());
        assert_eq!(cfg.guarded_agents, vec!["claude", "chatgpt"]);
        // 全垃圾 → 回填 claude（不静默裸奔）
        cfg.guarded_agents = vec!["nope".into()];
        assert!(cfg.migrate());
        assert_eq!(cfg.guarded_agents, default_guarded_agents());
        // 显式清空 → 尊重用户
        cfg.guarded_agents = Vec::new();
        assert!(!cfg.migrate(), "清空是合法状态，不回填");
        assert!(cfg.guarded_agents.is_empty());
    }
}
