//! Tauri 명령. 모든 입력은 신뢰하지 않고 Core에서 검증한다.

use crate::core::{Core, Overview};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::modules::{self, backup};
use crate::platform;
use std::sync::Arc;
use tauri::{AppHandle, State};

type CoreState<'a> = State<'a, Arc<Core>>;

fn check_id(id: &str) -> AppResult<()> {
    if id.len() > 64 || id.is_empty() {
        return Err(AppError::invalid("잘못된 식별자"));
    }
    Ok(())
}

#[tauri::command]
pub fn get_overview(app: AppHandle, core: CoreState) -> AppResult<Overview> {
    core.overview(platform::autostart_is_enabled(&app))
}

#[tauri::command]
pub fn set_master(core: CoreState, enabled: bool) -> AppResult<()> {
    core.set_master(enabled)
}

#[tauri::command]
pub fn set_background_mode(app: AppHandle, core: CoreState, enabled: bool) -> AppResult<()> {
    // 백그라운드를 끄면 로그인 자동 실행도 의미가 없으므로 함께 해제한다(사용자에게 UI에서 안내).
    if !enabled && platform::autostart_is_enabled(&app).0 {
        platform::set_autostart(&app, false)?;
    }
    core.set_background_mode(enabled)
}

/// Launch at Login. 백그라운드 상주가 꺼져 있으면 `alsoEnableBackground`가 true일 때만 함께 켠다.
#[tauri::command]
pub fn set_launch_at_login(app: AppHandle, core: CoreState, enabled: bool, also_enable_background: bool) -> AppResult<bool> {
    if enabled && !core.runtime().background {
        if !also_enable_background {
            return Err(AppError::new("NEEDS_BACKGROUND", "로그인 시 자동 실행하려면 백그라운드 상주가 켜져 있어야 합니다"));
        }
        core.set_background_mode(true)?;
    }
    let actual = platform::set_autostart(&app, enabled)?;
    db::set_setting(&core.db(), "launch_at_login_enabled", &actual)?;
    core.notify_changed();
    Ok(actual)
}

#[tauri::command]
pub fn set_module_enabled(core: CoreState, id: String, enabled: bool) -> AppResult<()> {
    check_id(&id)?;
    core.set_module_enabled(&id, enabled)
}

#[tauri::command]
pub fn set_module_config(core: CoreState, id: String, config: serde_json::Value) -> AppResult<()> {
    check_id(&id)?;
    core.set_module_config(&id, config)
}

#[tauri::command]
pub fn list_watch_targets(core: CoreState, module_id: Option<String>) -> AppResult<Vec<db::WatchTarget>> {
    db::list_watch_targets(&core.db(), module_id.as_deref())
}

#[tauri::command]
pub fn add_watch_target(core: CoreState, module_id: String, path: String, recursive: bool) -> AppResult<String> {
    check_id(&module_id)?;
    if path.len() > 1024 {
        return Err(AppError::invalid("경로가 너무 깁니다"));
    }
    core.add_watch_target(&module_id, &path, recursive)
}

#[tauri::command]
pub fn remove_watch_target(core: CoreState, id: String) -> AppResult<()> {
    check_id(&id)?;
    core.remove_watch_target(&id)
}

#[tauri::command]
pub fn list_activity(core: CoreState, result: Option<String>, module_id: Option<String>, limit: Option<i64>) -> AppResult<Vec<db::ActivityRow>> {
    db::list_activity(&core.db(), result.as_deref(), module_id.as_deref(), limit.unwrap_or(200).clamp(1, 1000))
}

#[tauri::command]
pub fn list_approvals(core: CoreState) -> AppResult<Vec<db::JobRow>> {
    db::jobs_with_status(&core.db(), &["WAITING_APPROVAL"], None)
}

#[tauri::command]
pub fn approve_job(core: CoreState, id: String, category: Option<String>) -> AppResult<()> {
    check_id(&id)?;
    core.approve_job(&id, category)
}

#[tauri::command]
pub fn reject_job(core: CoreState, id: String) -> AppResult<()> {
    check_id(&id)?;
    core.reject_job(&id)
}

#[tauri::command]
pub fn scan_downloads(core: CoreState) -> AppResult<usize> {
    core.scan_downloads()
}

#[tauri::command]
pub fn undo_move(core: CoreState, move_id: String) -> AppResult<String> {
    check_id(&move_id)?;
    core.undo_move(&move_id)
}

#[tauri::command]
pub fn list_backed_files(core: CoreState, query: Option<String>) -> AppResult<Vec<backup::BackedFile>> {
    backup::list_backed_files(&core.db(), query.as_deref())
}

#[tauri::command]
pub fn list_versions(core: CoreState, original_path: String) -> AppResult<Vec<backup::VersionRow>> {
    backup::list_versions(&core.db(), &original_path)
}

#[tauri::command]
pub fn diff_version(core: CoreState, id: String) -> AppResult<backup::DiffResult> {
    check_id(&id)?;
    let v = backup::get_version(&core.db(), &id)?;
    backup::diff_with_current(&v)
}

#[tauri::command]
pub fn restore_version(core: CoreState, id: String, mode: String) -> AppResult<String> {
    check_id(&id)?;
    core.restore_version(&id, &mode)
}

#[tauri::command]
pub fn get_settings(core: CoreState) -> AppResult<serde_json::Value> {
    let c = core.db();
    Ok(serde_json::json!({
        "notificationsEnabled": db::get_setting_or(&c, "notifications_enabled", true),
        "firstRunCompleted": db::get_setting_or(&c, "first_run_completed", false),
    }))
}

#[tauri::command]
pub fn set_setting(core: CoreState, key: String, value: serde_json::Value) -> AppResult<()> {
    // 허용된 키만 저장한다.
    match key.as_str() {
        "notifications_enabled" | "first_run_completed" => {
            let b = value.as_bool().ok_or_else(|| AppError::invalid("true/false 값이어야 합니다"))?;
            db::set_setting(&core.db(), &key, &b)?;
        }
        _ => return Err(AppError::invalid("변경할 수 없는 설정입니다")),
    }
    core.notify_changed();
    Ok(())
}

#[tauri::command]
pub fn ai_status() -> crate::ai::ClaudeStatus {
    crate::ai::status()
}

#[tauri::command]
pub fn uninstall_preview(core: CoreState) -> AppResult<crate::uninstall::UninstallPreview> {
    crate::uninstall::preview(&core)
}

#[tauri::command]
pub fn uninstall_execute(app: AppHandle, core: CoreState, keep_backups: bool, confirm: String) -> AppResult<String> {
    if confirm != "제거" {
        return Err(AppError::invalid("확인 문구가 일치하지 않습니다"));
    }
    let msg = crate::uninstall::execute(&core, keep_backups)?;
    let app2 = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(400));
        app2.exit(0);
    });
    Ok(msg)
}

#[tauri::command]
pub fn engine_debug(core: CoreState) -> serde_json::Value {
    serde_json::json!({ "activeWatches": core.active_watch_count(), "modules": modules::REGISTRY.len() })
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    crate::quit(&app);
}
