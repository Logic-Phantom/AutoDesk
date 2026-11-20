//! 전역 단축키(클립보드 정리 / AI 다듬기 / 화면 캡처). 자동화 단축키는 모듈이 실행 중일 때만,
//! 캡처 단축키는 도구 설정에서 켜 둔 동안만 OS에 등록한다.
//! 클립보드는 단축키를 누른 순간에만 읽는다(상시 폴링 없음).

use crate::core::Core;
use crate::modules::{self, autopaste};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

#[derive(Default)]
pub struct Registered {
    pub paste: Option<Shortcut>,
    pub ai: Option<Shortcut>,
    pub capture: Option<Shortcut>,
    pub busy: bool,
}

pub type ShortcutRegistry = Mutex<Registered>;

pub fn unregister_all(app: &AppHandle) {
    let _ = app.global_shortcut().unregister_all();
    if let Some(s) = app.try_state::<ShortcutRegistry>() {
        let mut r = s.lock().unwrap_or_else(|e| e.into_inner());
        r.paste = None;
        r.ai = None;
        r.capture = None;
    }
}

fn parse(s: &str) -> Result<Shortcut, String> {
    Shortcut::from_str(s).map_err(|e| format!("단축키 형식 오류({s}): {e}"))
}

/// 현재 설정과 상태에 맞게 단축키를 다시 등록한다. 등록 실패는 성공처럼 보이지 않게 오류로 남긴다.
pub fn sync(core: &Arc<Core>) {
    let app = &core.app;
    unregister_all(app);
    core.update_runtime(|r| {
        r.shortcut_error = None;
        r.ai_shortcut_error = None;
        r.capture_shortcut_error = None;
    });
    let rt = core.runtime();
    if rt.shutting_down {
        return;
    }
    // 화면 캡처는 자동화가 아니라 도구이므로 전체 자동화 스위치와 무관하게 동작한다.
    let tools = crate::tools::tool_settings(core);
    if tools.capture_shortcut_enabled {
        if let Some(state) = app.try_state::<ShortcutRegistry>() {
            match parse(&tools.capture_shortcut).and_then(|s| app.global_shortcut().register(s).map(|_| s).map_err(|e| e.to_string())) {
                Ok(s) => state.lock().unwrap_or_else(|e| e.into_inner()).capture = Some(s),
                Err(e) => core.update_runtime(|r| r.capture_shortcut_error = Some(format!("캡처 단축키 등록 실패: {e}"))),
            }
        }
    }
    if !rt.master {
        return;
    }
    let row = match crate::db::get_module(&core.db(), modules::AUTOPASTE) {
        Ok(r) => r,
        Err(_) => return,
    };
    if !row.enabled || row.consecutive_errors >= 5 {
        return;
    }
    let cfg: autopaste::AutoPasteConfig = serde_json::from_str(&row.config_json).unwrap_or_default();
    let Some(state) = app.try_state::<ShortcutRegistry>() else { return };

    match parse(&cfg.shortcut).and_then(|s| app.global_shortcut().register(s).map(|_| s).map_err(|e| e.to_string())) {
        Ok(s) => state.lock().unwrap_or_else(|e| e.into_inner()).paste = Some(s),
        Err(e) => core.update_runtime(|r| r.shortcut_error = Some(format!("단축키 등록 실패 — 다른 앱과 충돌했을 수 있습니다: {e}"))),
    }
    if cfg.ai_enabled && cfg.ai_consent {
        match parse(&cfg.ai_shortcut).and_then(|s| app.global_shortcut().register(s).map(|_| s).map_err(|e| e.to_string())) {
            Ok(s) => state.lock().unwrap_or_else(|e| e.into_inner()).ai = Some(s),
            Err(e) => core.update_runtime(|r| r.ai_shortcut_error = Some(format!("AI 단축키 등록 실패: {e}"))),
        }
    }
}

/// global-shortcut 플러그인 핸들러에서 호출된다.
pub fn on_shortcut(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state() != ShortcutState::Pressed {
        return;
    }
    let (Some(core), Some(state)) = (app.try_state::<Arc<Core>>(), app.try_state::<ShortcutRegistry>()) else { return };
    let core = Arc::clone(&core);
    let kind = {
        let mut r = state.lock().unwrap_or_else(|e| e.into_inner());
        if r.capture.as_ref() == Some(shortcut) {
            let app = app.clone();
            std::thread::spawn(move || {
                if let Err(e) = crate::tools::capture::start(&app, "region") {
                    core.notify("캡처 실패", &e.message);
                }
            });
            return;
        }
        let kind = if r.paste.as_ref() == Some(shortcut) {
            "paste"
        } else if r.ai.as_ref() == Some(shortcut) {
            "ai"
        } else {
            return;
        };
        if r.busy {
            return; // 이전 작업이 끝나지 않았으면 무시
        }
        r.busy = true;
        kind
    };
    let app = app.clone();
    std::thread::spawn(move || {
        match kind {
            "paste" => run_clean(&core),
            _ => run_ai(&core),
        }
        if let Some(state) = app.try_state::<ShortcutRegistry>() {
            state.lock().unwrap_or_else(|e| e.into_inner()).busy = false;
        }
        core.notify_changed();
    });
}

fn read_text(core: &Core) -> Option<String> {
    match core.app.clipboard().read_text() {
        Ok(t) if !t.is_empty() => Some(t),
        _ => {
            core.notify("클립보드 정리", "클립보드에 텍스트가 없습니다(이미지·파일은 건드리지 않습니다)");
            None
        }
    }
}

fn run_clean(core: &Arc<Core>) {
    if !core.is_module_running(modules::AUTOPASTE) {
        return;
    }
    let cfg: autopaste::AutoPasteConfig = core.module_config(modules::AUTOPASTE);
    let Some(text) = read_text(core) else { return };
    let (cleaned, stats) = autopaste::clean_text(&text, &cfg);
    drop(text);
    let (result, msg) = if stats.total() == 0 {
        ("skipped", "정리할 내용이 없습니다".to_string())
    } else {
        match core.app.clipboard().write_text(cleaned) {
            Ok(()) => ("completed", format!("{}곳 정리{}", stats.total(), if stats.code_mode { " (코드 모드)" } else { "" })),
            Err(e) => ("failed", format!("클립보드 쓰기 실패: {e}")),
        }
    };
    // 이력에는 건수만 남긴다. 텍스트는 저장하지 않는다.
    let _ = crate::db::add_activity(&core.db(), modules::AUTOPASTE, "clipboard.clean", result, None, &msg, None, None);
    let _ = crate::db::record_module_run(&core.db(), modules::AUTOPASTE, None);
    if cfg.notify {
        core.notify("클립보드 정리", &msg);
    }
}

fn run_ai(core: &Arc<Core>) {
    if !core.is_module_running(modules::AUTOPASTE) {
        return;
    }
    let cfg: autopaste::AutoPasteConfig = core.module_config(modules::AUTOPASTE);
    if !(cfg.ai_enabled && cfg.ai_consent) {
        return;
    }
    let Some(text) = read_text(core) else { return };
    core.notify("AI 다듬기", "Claude Code로 다듬는 중…");
    let work = core.paths.temp.join("ai-work");
    let (result, msg) = match crate::ai::polish(&text, &cfg.ai_model, &work) {
        Ok(out) if out == text => ("skipped", "고칠 부분이 없습니다".to_string()),
        Ok(out) => match core.app.clipboard().write_text(out) {
            Ok(()) => ("completed", "다듬은 글을 클립보드에 넣었습니다. 붙여넣기 하세요".to_string()),
            Err(e) => ("failed", format!("클립보드 쓰기 실패: {e}")),
        },
        Err(e) => ("failed", e.message),
    };
    let _ = crate::db::add_activity(&core.db(), modules::AUTOPASTE, "clipboard.ai_polish", result, None, &msg, None, None);
    let _ = crate::db::record_module_run(&core.db(), modules::AUTOPASTE, None);
    core.notify("AI 다듬기", &msg);
}
