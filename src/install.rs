//! macOS login launch agent. Does not modify Clash or require administrator rights.
use std::path::PathBuf;
fn plist_path() -> Result<PathBuf, String> {
    Ok(
        PathBuf::from(std::env::var_os("HOME").ok_or("HOME 不可用")?)
            .join("Library/LaunchAgents/local.claudeguard.mac.plist"),
    )
}
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let path = plist_path()?;
    if !enabled {
        match std::fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.to_string()),
        }
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if !exe.to_string_lossy().contains(".app/Contents/MacOS/") {
        return Err("请先从打包的 .app 运行，再开启登录自启".into());
    }
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let text = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>Label</key><string>local.claudeguard.mac</string><key>ProgramArguments</key><array><string>{}</string><string>--tray</string></array><key>RunAtLoad</key><true/></dict></plist>"#,
        xml_escape(&exe.to_string_lossy())
    );
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    // No launchctl bootstrap: enabling login start must not launch a second guard now.
    Ok(())
}
