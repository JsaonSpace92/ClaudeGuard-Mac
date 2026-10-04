mod checks;
mod config;
mod guard;
mod install;
mod ipc;
mod monitor;

use monitor::{Purpose, Shared};
use std::sync::{atomic::Ordering, Arc};
use tauri::Manager;

// A management window is disposable; monitoring lives in Shared, outside WebKit.
fn show_manager(app: &tauri::AppHandle) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window("main") {
        w.show()?;
        return w.set_focus();
    }
    let sh = app.state::<Arc<Shared>>().inner().clone();
    let win = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
        .title("ClaudeGuard Mac · AI 出口守护")
        .inner_size(520.0, 760.0)
        .min_inner_size(470.0, 580.0)
        .icon(tauri::image::Image::from_bytes(include_bytes!(
            "../assets/icon.png"
        ))?)?
        .build()?;
    let handle = app.clone();
    win.on_window_event(move |e| {
        if let tauri::WindowEvent::CloseRequested { .. } = e {
            if !sh.cfg.lock().unwrap().close_to_tray {
                sh.stop.store(true, Ordering::SeqCst);
                sh.wake_monitor();
                handle.exit(0);
            }
            // Otherwise allow destruction: the tray and Rust monitor keep running.
        }
    });
    win.set_focus()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--check") {
        let cfg = config::Config::load();
        let out = checks::run_checks(&cfg, |_| {});
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
        std::process::exit(if out.passed { 0 } else { 2 });
    }
    if args.iter().any(|a| a == "--list-apps") {
        println!(
            "{}",
            serde_json::to_string_pretty(&guard::app_status()).unwrap()
        );
        return;
    }
    let mut cfg = config::Config::load();
    cfg.egress_region.clear();
    cfg.quarantine_on_fail = false;
    if monitor::validate_config(&cfg).is_err() || cfg.first_run {
        cfg.armed = false;
    }
    let sh = Arc::new(Shared::new(cfg));
    let hidden = args.iter().any(|a| a == "--tray");
    let setup = sh.clone();
    let run = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let _ = show_manager(app);
        }))
        .manage(sh.clone())
        .invoke_handler(tauri::generate_handler![
            ipc::get_state,
            ipc::get_config,
            ipc::set_config,
            ipc::recheck,
            ipc::launch,
            ipc::app_status,
            ipc::recent_logs,
            ipc::open_data_folder,
            ipc::set_autostart
        ])
        .setup(move |app| {
            setup.attach(app.handle().clone());
            let icon = tauri::image::Image::from_bytes(include_bytes!("../assets/tray-icon.png"))?;
            if !hidden {
                show_manager(app.handle())?;
            }
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
            let show = MenuItem::with_id(app, "show", "打开管理窗口", true, None::<&str>)?;
            let check = MenuItem::with_id(app, "check", "立即检测", true, None::<&str>)?;
            let stop =
                MenuItem::with_id(app, "stop", "停止守护（应用继续运行）", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出守护器", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &check, &stop, &quit])?;
            let state = setup.clone();
            TrayIconBuilder::with_id("main")
                .icon(icon)
                .icon_as_template(true)
                .tooltip("ClaudeGuard Mac")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "show" => {
                        if let Err(e) = show_manager(app) {
                            state.log(format!("打开窗口失败：{e}"));
                        }
                    }
                    "check" => {
                        if let Err(e) = monitor::spawn_check(&state, Purpose::Manual) {
                            state.log(e);
                        }
                    }
                    "stop" => {
                        let mut cfg = state.cfg.lock().unwrap().clone();
                        cfg.armed = false;
                        if let Err(e) = state.replace_config(cfg) {
                            state.log(e);
                        } else {
                            state.log("守护已停止，目标应用仍可运行");
                        }
                    }
                    "quit" => {
                        state.stop.store(true, Ordering::SeqCst);
                        state.wake_monitor();
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let _ = show_manager(tray.app_handle());
                    }
                })
                .build(app)?;
            setup.log("macOS 版就绪：仅检测出口并关闭应用，不设置防火墙、不修改 Clash");
            monitor::spawn_monitor(setup.clone());
            Ok(())
        })
        .build(tauri::generate_context!());
    let lifecycle = sh.clone();
    let run = run.map(|app| {
        app.run(move |handle, event| match event {
            // Closing the last management window must not stop the guard.
            tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } if !lifecycle.stop.load(Ordering::SeqCst) => api.prevent_exit(),
            tauri::RunEvent::Reopen { .. } => {
                let _ = show_manager(handle);
            }
            _ => {}
        });
    });
    sh.stop.store(true, Ordering::SeqCst);
    sh.wake_monitor();
    if let Err(e) = run {
        eprintln!("启动失败: {e}");
    }
}
