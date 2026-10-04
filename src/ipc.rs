use crate::{
    config::Config,
    guard,
    monitor::{self, Purpose, Shared},
};
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
