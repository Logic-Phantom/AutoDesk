//! AutoDesk — 개인용 PC 상주 자동화 엔진.
//!
//! 프로세스는 하나뿐이다. 창을 닫으면(백그라운드 상주 ON) 트레이로 숨고, OFF면 앱이 정상 종료된다.
//! 종료 시 감시자·단축키·엔진 스레드가 모두 정리되므로 남는 백그라운드 프로세스가 없다.

mod ai;
mod commands;
mod core;
mod db;
mod error;
mod fsutil;
mod modules;
mod platform;
mod shortcuts;
mod tray;
mod uninstall;

use crate::core::Core;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub(crate) fn create_main_window(app: &AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("AutoDesk")
        .inner_size(1080.0, 720.0)
        .min_inner_size(820.0, 560.0)
        .visible(false)
        .build()
}

/// 트레이 '프로그램 종료' 등: 엔진을 정리한 뒤 프로세스를 끝낸다.
pub(crate) fn quit(app: &AppHandle) {
    if let Some(core) = app.try_state::<Arc<Core>>() {
        core.shutdown();
    }
    app.exit(0);
}

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    let started_by_autostart = args.iter().any(|a| a == platform::AUTOSTART_ARG);
    let quit_requested = args.iter().any(|a| a == platform::QUIT_ARG);

    tauri::Builder::default()
        // 반드시 첫 번째 플러그인: 두 번째 실행은 기존 프로세스로 전달하고 바로 끝난다.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if argv.iter().any(|a| a == platform::QUIT_ARG) {
                // 설치 제거 프로그램이 정상 종료를 요청한 경우
                quit(app);
                return;
            }
            tray::show_main(app, None);
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name(platform::AUTOSTART_NAME)
                .args([platform::AUTOSTART_ARG])
                .build(),
        )
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| shortcuts::on_shortcut(app, shortcut, event))
                .build(),
        )
        .manage::<shortcuts::ShortcutRegistry>(Mutex::new(shortcuts::Registered::default()))
        .setup(move |app| {
            if quit_requested {
                // 실행 중인 인스턴스가 없을 때 --quit: 아무 것도 만들지 않고 끝낸다.
                app.handle().exit(0);
                return Ok(());
            }
            let handle = app.handle().clone();
            let core = Core::init(&handle).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            app.manage(Arc::clone(&core));
            tray::create(&handle)?;
            let window = create_main_window(&handle)?;
            core.start();
            if !started_by_autostart {
                window.show()?;
                let _ = window.set_focus();
                core.update_runtime(|r| r.window_visible = true);
            }
            core.notify_changed();
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let Some(core) = app.try_state::<Arc<Core>>().map(|c| Arc::clone(&c)) else { return };
                if core.runtime().background && !core.runtime().shutting_down {
                    // 백그라운드 상주: 창만 숨기고 자동화는 계속
                    api.prevent_close();
                    let _ = window.hide();
                    let first = !core.runtime().tray_hint_shown;
                    core.update_runtime(|r| {
                        r.window_visible = false;
                        r.tray_hint_shown = true;
                    });
                    if first {
                        core.notify("AutoDesk는 계속 실행 중", "트레이 아이콘에서 열거나 종료할 수 있습니다.");
                    }
                    core.notify_changed();
                } else {
                    // 백그라운드 OFF: 창 닫기 = 앱 종료
                    core.shutdown();
                    app.exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_overview,
            commands::set_master,
            commands::set_background_mode,
            commands::set_launch_at_login,
            commands::set_module_enabled,
            commands::set_module_config,
            commands::list_watch_targets,
            commands::add_watch_target,
            commands::remove_watch_target,
            commands::list_activity,
            commands::list_approvals,
            commands::approve_job,
            commands::reject_job,
            commands::scan_downloads,
            commands::undo_move,
            commands::list_backed_files,
            commands::list_versions,
            commands::diff_version,
            commands::restore_version,
            commands::get_settings,
            commands::set_setting,
            commands::list_log_issues,
            commands::clear_log_issues,
            commands::ai_status,
            commands::uninstall_preview,
            commands::uninstall_execute,
            commands::engine_debug,
            commands::quit_app,
        ])
        .build(tauri::generate_context!())
        .expect("AutoDesk 실행 실패")
        .run(|app, event| match event {
            tauri::RunEvent::ExitRequested { code, api, .. } => {
                // 창이 모두 닫혀서 생긴 종료 요청(code=None)은 백그라운드 상주 중이면 막는다.
                if code.is_none() {
                    if let Some(core) = app.try_state::<Arc<Core>>() {
                        if core.runtime().background && !core.runtime().shutting_down {
                            api.prevent_exit();
                        }
                    }
                }
            }
            tauri::RunEvent::Exit => {
                if let Some(core) = app.try_state::<Arc<Core>>() {
                    core.shutdown();
                }
            }
            _ => {}
        });
}
