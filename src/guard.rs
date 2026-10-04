//! macOS application matching uses executable paths inside registered .app bundles.
use crate::config::Config;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use sysinfo::{ProcessRefreshKind, Signal, System, UpdateKind};

#[derive(Clone, Serialize)]
pub struct AppInfo {
    pub id: String,
    pub name: String,
    pub path: Option<String>,
    pub running: usize,
}
pub fn gui_agent_ids() -> Vec<&'static str> {
    vec!["claude", "chatgpt", "antigravity"]
}
pub fn gui_agent_name(id: &str) -> &str {
    match id {
        "claude" => "Claude",
        "chatgpt" => "ChatGPT",
        "antigravity" => "Antigravity",
        _ => id,
    }
}
fn valid_bundle(id: &str, bundle: &str) -> bool {
    match id {
        "claude" => matches!(
            bundle,
            "com.anthropic.claudefordesktop" | "com.anthropic.claude"
        ),
        "chatgpt" => matches!(
            bundle,
            "com.openai.chat" | "com.openai.codex" | "com.openai.chatgpt"
        ),
        "antigravity" => bundle == "com.google.antigravity",
        _ => false,
    }
}

pub fn find_app(id: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    for base in [PathBuf::from("/Applications"), home.join("Applications")] {
        let Ok(entries) = std::fs::read_dir(base) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("app") {
                continue;
            }
            // Filter names before reading metadata to keep recurring scans cheap.
            if !path
                .file_name()?
                .to_string_lossy()
                .to_lowercase()
                .contains(id)
            {
                continue;
            }
            let output = Command::new("/usr/bin/plutil")
                .args(["-extract", "CFBundleIdentifier", "raw", "-o", "-"])
                .arg(path.join("Contents/Info.plist"))
                .output()
                .ok()?;
            if output.status.success()
                && valid_bundle(id, String::from_utf8_lossy(&output.stdout).trim())
            {
                return path.canonicalize().ok();
            }
        }
    }
    None
}

pub fn matches_root(exe: &Path, bundle: &Path) -> bool {
    // Component-aware: /Applications/ChatGPT.app.evil cannot match ChatGPT.app.
    let contents = bundle.join("Contents");
    exe.starts_with(&contents)
        || exe
            .canonicalize()
            .map(|p| p.starts_with(&contents))
            .unwrap_or(false)
}

fn snapshot() -> System {
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessRefreshKind::new().with_exe(UpdateKind::Always));
    system
}

pub fn app_status() -> Vec<AppInfo> {
    let system = snapshot();
    gui_agent_ids()
        .into_iter()
        .map(|id| {
            let root = find_app(id);
            let running = root
                .as_ref()
                .map(|r| {
                    system
                        .processes()
                        .values()
                        .filter(|p| p.exe().map(|e| matches_root(e, r)).unwrap_or(false))
                        .count()
                })
                .unwrap_or(0);
            AppInfo {
                id: id.into(),
                name: gui_agent_name(id).into(),
                path: root.map(|r| r.to_string_lossy().into()),
                running,
            }
        })
        .collect()
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct TripResult {
    pub killed: usize,
    pub remaining: usize,
    pub agents: Vec<String>,
}

/// Refresh each PID before signaling: never match a generic node/python process name.
fn terminate_bundles(roots: &[(String, PathBuf)]) -> TripResult {
    let mut result = TripResult::default();
    if roots.is_empty() {
        return result;
    }
    // One process snapshot for all selected apps; avoid full CPU/memory metadata scans.
    let mut sys = snapshot();
    let targets: Vec<_> = sys
        .processes()
        .iter()
        .filter_map(|(pid, p)| {
            let exe = p.exe()?;
            roots
                .iter()
                .position(|(_, root)| matches_root(exe, root))
                .map(|index| (*pid, p.start_time(), index))
        })
        .collect();
    if targets.is_empty() {
        return result;
    }
    let mut affected = vec![false; roots.len()];
    for (pid, started, index) in targets {
        if !sys
            .refresh_process_specifics(pid, ProcessRefreshKind::new().with_exe(UpdateKind::Always))
        {
            continue;
        }
        let Some(p) = sys.process(pid) else {
            continue;
        };
        if p.start_time() != started
            || !p
                .exe()
                .map(|e| matches_root(e, &roots[index].1))
                .unwrap_or(false)
        {
            continue;
        }
        if p.kill_with(Signal::Kill).unwrap_or(false) {
            result.killed += 1;
            affected[index] = true;
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(100));
    sys.refresh_processes_specifics(ProcessRefreshKind::new().with_exe(UpdateKind::Always));
    for process in sys.processes().values() {
        if let Some(index) = process
            .exe()
            .and_then(|exe| roots.iter().position(|(_, root)| matches_root(exe, root)))
        {
            result.remaining += 1;
            affected[index] = true;
        }
    }
    result.agents = roots
        .iter()
        .enumerate()
        .filter(|(i, _)| affected[*i])
        .map(|(_, (id, _))| gui_agent_name(id).to_string())
        .collect();
    result
}

pub fn trip(cfg: &Config) -> TripResult {
    if !cfg.kill_on_fail {
        return TripResult::default();
    }
    let roots: Vec<_> = cfg
        .guarded_agents
        .iter()
        .filter_map(|id| find_app(id).map(|root| (id.clone(), root)))
        .collect();
    // The monitor logs state changes; no duplicate disk write on every failed cycle.
    terminate_bundles(&roots)
}

pub fn launch_agent(id: &str) -> Result<(), String> {
    let path =
        find_app(id).ok_or_else(|| format!("未在 Applications 中找到 {}", gui_agent_name(id)))?;
    let status = Command::new("/usr/bin/open")
        .arg("-a")
        .arg(path)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("macOS 启动失败".into())
    }
}

pub fn log_line(msg: &str) {
    use std::io::Write;
    let dir = crate::config::config_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("guard.log");
    if std::fs::metadata(&path)
        .map(|m| m.len() > 1_000_000)
        .unwrap_or(false)
    {
        let _ = std::fs::rename(&path, dir.join("guard.previous.log"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = writeln!(f, "{now} {msg}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn helper_paths_match_without_name_collateral() {
        let root = Path::new("/Applications/ChatGPT.app");
        assert!(matches_root(
            Path::new(
                "/Applications/ChatGPT.app/Contents/Frameworks/Helper.app/Contents/MacOS/Helper"
            ),
            root
        ));
        assert!(!matches_root(
            Path::new("/Applications/ChatGPT.app.evil/Contents/MacOS/ChatGPT"),
            root
        ));
        assert!(!matches_root(Path::new("/usr/local/bin/node"), root));
    }
    #[test]
    fn bundle_ids_are_exact() {
        assert!(valid_bundle("chatgpt", "com.openai.codex"));
        assert!(!valid_bundle("chatgpt", "com.openai.codex.evil"));
        assert!(!valid_bundle("antigravity", "com.google.Chrome"));
    }
    #[test]
    fn termination_only_targets_owned_test_bundle() {
        let root = std::env::temp_dir().join(format!("cg-owned-{}.app", std::process::id()));
        let bin = root.join("Contents/MacOS");
        std::fs::create_dir_all(&bin).unwrap();
        let root = root.canonicalize().unwrap();
        let sleeper = bin.join("owned-sleep");
        use std::io::Write;
        let mut cc = Command::new("/usr/bin/clang")
            .args(["-x", "c", "-", "-o"])
            .arg(&sleeper)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        cc.stdin
            .take()
            .unwrap()
            .write_all(b"#include <unistd.h>\nint main(void) { for (;;) sleep(1); }\n")
            .unwrap();
        assert!(cc.wait().unwrap().success());
        let second_root =
            root.with_file_name(format!("cg-owned-second-{}.app", std::process::id()));
        let second_bin = second_root.join("Contents/MacOS/owned-sleep");
        std::fs::create_dir_all(second_bin.parent().unwrap()).unwrap();
        std::fs::copy(&sleeper, &second_bin).unwrap();
        let mut second_owned = Command::new(&second_bin).spawn().unwrap();
        let mut owned = Command::new(&sleeper).arg("60").spawn().unwrap();
        let mut unrelated = Command::new("/bin/sleep").arg("60").spawn().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        let result = terminate_bundles(&[
            ("test".into(), root.clone()),
            ("second".into(), second_root.clone()),
        ]);
        let stopped = owned.try_wait().unwrap().is_some();
        let second_stopped = second_owned.try_wait().unwrap().is_some();
        let untouched = unrelated.try_wait().unwrap().is_none();
        let _ = owned.kill();
        let _ = owned.wait();
        let _ = second_owned.kill();
        let _ = second_owned.wait();
        let _ = unrelated.kill();
        let _ = unrelated.wait();
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&second_root);
        assert_eq!(result.killed, 2);
        assert!(second_stopped);
        assert!(stopped);
        assert!(untouched);
    }
}
