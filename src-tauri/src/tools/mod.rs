//! 필요할 때만 쓰는 도구들(백그라운드에서 돌지 않음): 화면 캡처, 표 변환, 문서 비교, 이미지 변환
//! 그리고 업무 모드 플로팅 버튼(dock).

pub mod capture;
pub mod files;
pub mod images;
pub mod ocr;

use crate::core::Core;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::workspace;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindowBuilder};

type CoreState<'a> = State<'a, Arc<Core>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ToolSettings {
    pub capture_shortcut: String,
    pub capture_shortcut_enabled: bool,
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self { capture_shortcut: "CommandOrControl+Alt+Shift+KeyS".into(), capture_shortcut_enabled: true }
    }
}

pub fn tool_settings(core: &Core) -> ToolSettings {
    db::get_setting_or(&core.db(), "tools", ToolSettings::default())
}

// ---------------- 업무 모드 플로팅 버튼 ----------------

const DOCK_MIN: (f64, f64) = (64.0, 64.0);
const DOCK_MARGIN: f64 = 18.0;

/// 주 모니터 작업 영역(작업 표시줄 제외)의 오른쪽 아래에 붙인다. 크기를 바꿔도 오른쪽 아래 모서리는 고정.
fn place_dock(app: &AppHandle, width: f64, height: f64) {
    let Some(w) = app.get_webview_window("dock") else { return };
    let monitor = w.primary_monitor().ok().flatten().or_else(|| w.current_monitor().ok().flatten());
    let Some(m) = monitor else { return };
    let scale = m.scale_factor();
    let (pw, ph) = ((width.clamp(48.0, 640.0) * scale) as u32, (height.clamp(48.0, 640.0) * scale) as u32);
    let area = m.work_area();
    let x = area.position.x + area.size.width as i32 - pw as i32 - (DOCK_MARGIN * scale) as i32;
    let y = area.position.y + area.size.height as i32 - ph as i32 - (DOCK_MARGIN * scale) as i32;
    let _ = w.set_size(PhysicalSize::new(pw, ph));
    let _ = w.set_position(PhysicalPosition::new(x, y));
}

pub fn sync_dock(core: &Arc<Core>) {
    let app = &core.app;
    let cfg = workspace::load(core);
    let existing = app.get_webview_window("dock");
    if !cfg.dock_enabled || core.runtime().shutting_down {
        if let Some(w) = existing {
            let _ = w.destroy(); // close()는 닫기 방지 처리에 막히므로 destroy
        }
        return;
    }
    if existing.is_some() {
        return;
    }
    let built = WebviewWindowBuilder::new(app, "dock", WebviewUrl::App("index.html#/dock".into()))
        .title("AutoDesk 업무 모드")
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focused(false)
        .visible(false)
        .inner_size(DOCK_MIN.0, DOCK_MIN.1)
        .build();
    if built.is_ok() {
        place_dock(app, DOCK_MIN.0, DOCK_MIN.1);
        if let Some(w) = app.get_webview_window("dock") {
            let _ = w.show();
        }
    }
}

// ---------------- 명령 ----------------

#[tauri::command]
pub fn dock_set_size(app: AppHandle, width: f64, height: f64) {
    place_dock(&app, width, height);
}

#[tauri::command]
pub fn workspace_get(core: CoreState) -> workspace::WorkspaceConfig {
    workspace::load(&core)
}

// 주의: Windows에서는 동기 명령(메인 스레드)에서 창을 만들면 교착된다. 창을 만들거나 닫는 명령은 async로 둔다.
#[tauri::command]
pub async fn workspace_set(core: CoreState<'_>, config: workspace::WorkspaceConfig) -> AppResult<()> {
    let c = workspace::validate(config)?;
    db::set_setting(&core.db(), "workspace", &c)?;
    sync_dock(&core);
    let _ = core.app.emit("workspace://changed", ());
    Ok(())
}

static MODE_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// mode: start | away | end. 오래 걸릴 수 있으므로 별도 스레드에서 실행. 동시에 두 번 실행하지 않는다.
#[tauri::command]
pub async fn workspace_run(core: CoreState<'_>, mode: String) -> AppResult<workspace::ModeResult> {
    use std::sync::atomic::Ordering;
    if MODE_RUNNING.swap(true, Ordering::SeqCst) {
        return Err(AppError::new("BUSY", "다른 업무 모드가 아직 실행 중입니다. 끝난 뒤 다시 눌러 주세요"));
    }
    let core = Arc::clone(&core);
    let r = tauri::async_runtime::spawn_blocking(move || match mode.as_str() {
        "start" => Ok(workspace::run_start(&core)),
        "away" => Ok(workspace::run_away(&core)),
        "end" => Ok(workspace::run_end(&core)),
        _ => Err(AppError::invalid("알 수 없는 모드")),
    })
    .await
    .map_err(|e| AppError::new("TASK_FAILED", e.to_string()));
    MODE_RUNNING.store(false, Ordering::SeqCst);
    r?
}

#[tauri::command]
pub fn workspace_preview_end(core: CoreState) -> Vec<workspace::platform::AppWindow> {
    workspace::preview_end(&core)
}

#[tauri::command]
pub fn workspace_running_apps() -> Vec<workspace::platform::AppWindow> {
    let mut v = workspace::platform::list_windows();
    v.retain(|w| !workspace::is_protected(&w.process));
    v.sort_by(|a, b| a.process.to_lowercase().cmp(&b.process.to_lowercase()));
    v
}

#[tauri::command]
pub fn workspace_cancel_shutdown() -> AppResult<String> {
    workspace::platform::cancel_shutdown().map_err(|e| AppError::new("SHUTDOWN_CANCEL", e))
}

#[tauri::command]
pub async fn open_main(app: AppHandle, route: Option<String>) -> AppResult<()> {
    crate::tray::show_main(&app, route.as_deref());
    Ok(())
}

#[tauri::command]
pub fn tool_settings_get(core: CoreState) -> ToolSettings {
    tool_settings(&core)
}

#[tauri::command]
pub fn tool_settings_set(core: CoreState, settings: ToolSettings) -> AppResult<()> {
    db::set_setting(&core.db(), "tools", &settings)?;
    crate::shortcuts::sync(&core);
    core.notify_changed();
    Ok(())
}

// Capture

#[tauri::command]
pub async fn capture_start(app: AppHandle, mode: String) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || capture::start(&app, &mode))
        .await
        .map_err(|e| AppError::new("TASK_FAILED", e.to_string()))?
}

#[tauri::command]
pub fn capture_list_windows() -> AppResult<Vec<capture::WindowInfo>> {
    capture::list_windows()
}

#[tauri::command]
pub async fn capture_window(app: AppHandle, id: u32) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || capture::capture_window(&app, id))
        .await
        .map_err(|e| AppError::new("TASK_FAILED", e.to_string()))?
}

#[tauri::command]
pub async fn capture_crop(app: AppHandle, x: u32, y: u32, w: u32, h: u32) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || capture::crop(&app, x, y, w, h))
        .await
        .map_err(|e| AppError::new("TASK_FAILED", e.to_string()))?
}

#[tauri::command]
pub async fn capture_cancel(app: AppHandle) -> AppResult<()> {
    capture::clear(&app, None);
    if let Some(o) = app.get_webview_window("overlay") {
        let _ = o.close();
    }
    Ok(())
}

#[tauri::command]
pub fn capture_get(app: AppHandle) -> AppResult<tauri::ipc::Response> {
    let png = capture::get(&app).ok_or_else(|| AppError::not_found("캡처한 이미지가 없습니다"))?;
    Ok(tauri::ipc::Response::new(png))
}

#[tauri::command]
pub fn capture_clear(app: AppHandle, gen: Option<u64>) {
    capture::clear(&app, gen);
}

#[tauri::command]
pub fn capture_gen(app: AppHandle) -> u64 {
    capture::generation(&app)
}

#[tauri::command]
pub fn image_copy(app: AppHandle, data: String) -> AppResult<()> {
    capture::copy_to_clipboard(&app, &data)
}

/// 텍스트 복사는 웹뷰의 navigator.clipboard 대신 OS 클립보드로(창 포커스와 무관하게 동작)
#[tauri::command]
pub fn text_copy(app: AppHandle, text: String) -> AppResult<()> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    if text.len() > 50 * 1024 * 1024 {
        return Err(AppError::invalid("복사할 내용이 너무 큽니다"));
    }
    app.clipboard().write_text(text).map_err(|e| AppError::new("CLIPBOARD_FAILED", format!("클립보드 복사 실패: {e}")))
}

#[tauri::command]
pub fn image_save(path: String, data: String) -> AppResult<()> {
    files::save_base64(std::path::Path::new(&path), &data)
}

#[tauri::command]
pub async fn ocr_image(data: String) -> AppResult<String> {
    let png = capture::decode_b64(&data)?;
    tauri::async_runtime::spawn_blocking(move || ocr::recognize_png(&png))
        .await
        .map_err(|e| AppError::new("TASK_FAILED", e.to_string()))?
}

// Compare / Table

#[tauri::command]
pub async fn read_text_file(path: String) -> AppResult<String> {
    tauri::async_runtime::spawn_blocking(move || files::read_text(std::path::Path::new(&path)))
        .await
        .map_err(|e| AppError::new("TASK_FAILED", e.to_string()))?
}

#[tauri::command]
pub fn save_text_file(path: String, contents: String, bom: bool) -> AppResult<()> {
    files::save_text(std::path::Path::new(&path), &contents, bom)
}

// 이미지 변환

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ImageProgress {
    done: usize,
    total: usize,
    result: images::ImageResult,
}

#[tauri::command]
pub async fn image_batch(app: AppHandle, job: images::ImageJob) -> AppResult<Vec<images::ImageResult>> {
    if job.files.is_empty() {
        return Err(AppError::invalid("이미지를 선택하세요"));
    }
    if job.files.len() > 500 {
        return Err(AppError::invalid("한 번에 500개까지 처리할 수 있습니다"));
    }
    let out = images::output_dir(&job)?;
    tauri::async_runtime::spawn_blocking(move || {
        let total = job.files.len();
        let mut results = Vec::with_capacity(total);
        for (i, f) in job.files.iter().enumerate() {
            let r = images::process_one(&PathBuf::from(f), &out, &job);
            let _ = app.emit("tools://image-progress", ImageProgress { done: i + 1, total, result: r.clone() });
            results.push(r);
        }
        results
    })
    .await
    .map_err(|e| AppError::new("TASK_FAILED", e.to_string()))
}

#[tauri::command]
pub fn open_path(app: AppHandle, path: String) -> AppResult<()> {
    use tauri_plugin_opener::OpenerExt;
    if !std::path::Path::new(&path).exists() {
        return Err(AppError::not_found("경로가 없습니다"));
    }
    app.opener().open_path(path, None::<&str>).map_err(|e| AppError::new("OPEN_FAILED", e.to_string()))
}
