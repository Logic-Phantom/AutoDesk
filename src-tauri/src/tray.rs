//! 시스템 트레이(Windows) / 메뉴 막대(macOS). 체크 상태는 항상 Core 실제 상태에서 다시 그린다.

use crate::core::Core;
use crate::modules;
use std::sync::Arc;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

pub const TRAY_ID: &str = "autodesk-tray";

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("AutoDesk")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle(), None);
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let core = app.try_state::<Arc<Core>>().map(|c| Arc::clone(&c));
    let (master, background, pending) = match &core {
        Some(c) => {
            let rt = c.runtime();
            let pending: i64 = c
                .db()
                .query_row("SELECT COUNT(*) FROM jobs WHERE status='WAITING_APPROVAL'", [], |r| r.get(0))
                .unwrap_or(0);
            (rt.master, rt.background, pending)
        }
        None => (true, true, 0),
    };
    let open = MenuItem::with_id(app, "open", "AutoDesk 열기", true, None::<&str>)?;
    let master_i = CheckMenuItem::with_id(app, "master", "전체 자동화", true, master, None::<&str>)?;
    let bg = MenuItem::with_id(
        app,
        "bg",
        if background { "백그라운드 상주: 켜짐" } else { "백그라운드 상주: 꺼짐(창 닫으면 종료)" },
        false,
        None::<&str>,
    )?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let mut module_items = Vec::new();
    for m in modules::REGISTRY {
        let enabled = core.as_ref().and_then(|c| crate::db::get_module(&c.db(), m.id).ok()).map(|r| r.enabled).unwrap_or(false);
        module_items.push(CheckMenuItem::with_id(app, format!("mod:{}", m.id), m.name, master, enabled, None::<&str>)?);
    }
    let sep2 = PredefinedMenuItem::separator(app)?;
    let approvals = MenuItem::with_id(app, "approvals", format!("확인 대기 {pending}건"), true, None::<&str>)?;
    let activity = MenuItem::with_id(app, "activity", "최근 처리 내역", true, None::<&str>)?;
    let capture = MenuItem::with_id(app, "capture", "화면 캡처(영역)", true, None::<&str>)?;
    let workspace = MenuItem::with_id(app, "workspace", "업무 모드 설정", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "설정", true, None::<&str>)?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "프로그램 종료", true, None::<&str>)?;

    let mut items: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = vec![&open, &master_i, &bg, &sep1];
    for i in &module_items {
        items.push(i);
    }
    items.extend([&sep2 as &dyn tauri::menu::IsMenuItem<tauri::Wry>, &capture, &workspace, &approvals, &activity, &settings, &sep3, &quit]);
    Menu::with_items(app, &items)
}

pub fn refresh(app: &AppHandle) {
    let app2 = app.clone();
    // 메뉴는 메인 스레드에서 다시 만든다.
    let _ = app.run_on_main_thread(move || {
        if let Some(tray) = app2.tray_by_id(TRAY_ID) {
            if let Ok(menu) = build_menu(&app2) {
                let _ = tray.set_menu(Some(menu));
            }
            if let Some(core) = app2.try_state::<Arc<Core>>() {
                let tip = match core.app_status() {
                    crate::core::AppStatus::GlobalPaused => "AutoDesk — 일시정지",
                    _ => "AutoDesk — 자동화 실행 중",
                };
                let _ = tray.set_tooltip(Some(tip));
            }
        }
    });
}

pub fn show_main(app: &AppHandle, route: Option<&str>) {
    if let Some(core) = app.try_state::<Arc<Core>>() {
        core.update_runtime(|r| r.window_visible = true);
        core.notify_changed();
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        if let Some(r) = route {
            let _ = app.emit("core://navigate", r);
        }
        return;
    }
    // 창이 닫혀 있으면 새로 만든다. 메인 스레드를 막지 않도록 별도 스레드에서(Windows 교착 방지)
    let app = app.clone();
    let route = route.map(|r| r.to_string());
    std::thread::spawn(move || {
        if let Ok(w) = crate::create_main_window(&app, route.as_deref()) {
            let _ = w.show();
            let _ = w.set_focus();
        }
    });
}

fn handle_menu(app: &AppHandle, id: &str) {
    let Some(core) = app.try_state::<Arc<Core>>().map(|c| Arc::clone(&c)) else { return };
    match id {
        "open" => show_main(app, Some("dashboard")),
        "approvals" => show_main(app, Some("approvals")),
        "activity" => show_main(app, Some("activity")),
        "settings" => show_main(app, Some("settings")),
        "workspace" => show_main(app, Some("workspace")),
        "capture" => {
            let app = app.clone();
            std::thread::spawn(move || {
                let _ = crate::tools::capture::start(&app, "region");
            });
        }
        "master" => {
            let on = !core.runtime().master;
            let _ = core.set_master(on);
        }
        "quit" => crate::quit(app),
        other => {
            if let Some(mid) = other.strip_prefix("mod:") {
                let enabled = crate::db::get_module(&core.db(), mid).map(|r| r.enabled).unwrap_or(false);
                let _ = core.set_module_enabled(mid, !enabled);
            }
        }
    }
    refresh(app);
}
