//! 화면 캡처: 화면 전체/영역/창 캡처 → 편집 창.
//! 캡처 이미지는 메모리에만 두고, 사용자가 저장/복사할 때만 밖으로 나간다(디스크 임시 파일 없음).

use crate::error::{AppError, AppResult};
use base64::Engine;
use serde::Serialize;
use std::io::Cursor;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

#[derive(Default)]
pub struct CaptureStore {
    pub png: Option<Vec<u8>>,
    /// 캡처할 때마다 증가. 이전 편집기가 닫히면서 최신 캡처를 지우지 않도록 구분한다.
    pub gen: u64,
    /// 편집기가 마지막으로 불러온 캡처의 세대. 편집기가 닫히면 이 세대일 때만 지운다.
    pub editor_gen: u64,
}

pub type CaptureState = Mutex<CaptureStore>;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub id: u32,
    pub title: String,
    pub app: String,
    pub width: u32,
    pub height: u32,
}

fn to_png(img: &image::RgbaImage) -> AppResult<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).map_err(|e| AppError::new("CAPTURE_ENCODE", e.to_string()))?;
    Ok(buf.into_inner())
}

fn store(app: &AppHandle, png: Vec<u8>) {
    if let Some(s) = app.try_state::<CaptureState>() {
        let mut st = s.lock().unwrap_or_else(|e| e.into_inner());
        st.png = Some(png);
        st.gen += 1;
    }
}

pub fn generation(app: &AppHandle) -> u64 {
    app.try_state::<CaptureState>().map(|s| s.lock().unwrap_or_else(|e| e.into_inner()).gen).unwrap_or(0)
}

/// 캡처 전에 AutoDesk 창들을 잠시 숨긴다(캡처에 찍히지 않게). 숨긴 창 라벨을 돌려준다.
fn hide_own_windows(app: &AppHandle) -> Vec<String> {
    let mut hidden = Vec::new();
    for (label, w) in app.webview_windows() {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
            hidden.push(label);
        }
    }
    if !hidden.is_empty() {
        std::thread::sleep(Duration::from_millis(280));
    }
    hidden
}

fn restore_windows(app: &AppHandle, labels: &[String], except: &[&str]) {
    for l in labels {
        if except.contains(&l.as_str()) {
            continue;
        }
        if let Some(w) = app.get_webview_window(l) {
            let _ = w.show();
        }
    }
}

fn monitor_under_cursor(app: &AppHandle) -> AppResult<xcap::Monitor> {
    let pos = app.cursor_position().ok();
    let m = match pos {
        Some(p) => xcap::Monitor::from_point(p.x as i32, p.y as i32).ok(),
        None => None,
    };
    match m {
        Some(m) => Ok(m),
        None => xcap::Monitor::all()
            .map_err(|e| AppError::new("CAPTURE_FAILED", e.to_string()))?
            .into_iter()
            .find(|m| m.is_primary().unwrap_or(false))
            .ok_or_else(|| AppError::new("CAPTURE_FAILED", "모니터를 찾을 수 없습니다")),
    }
}

/// mode: full | region
pub fn start(app: &AppHandle, mode: &str) -> AppResult<()> {
    let hidden = hide_own_windows(app);
    let result = (|| -> AppResult<xcap::Monitor> {
        let m = monitor_under_cursor(app)?;
        let img = m.capture_image().map_err(|e| AppError::new("CAPTURE_FAILED", format!("화면 캡처 실패: {e}")))?;
        store(app, to_png(&img)?);
        Ok(m)
    })();
    // 캡처가 끝나면 편집/선택 창을 띄우고, 메인 창은 원래대로 돌려둔다(대시보드는 그대로)
    restore_windows(app, &hidden, &["editor", "overlay"]);
    let monitor = result?;
    if mode == "region" {
        open_overlay(app, &monitor)
    } else {
        open_editor(app)
    }
}

pub fn list_windows() -> AppResult<Vec<WindowInfo>> {
    let me = std::process::id();
    Ok(xcap::Window::all()
        .map_err(|e| AppError::new("CAPTURE_FAILED", e.to_string()))?
        .into_iter()
        .filter(|w| !w.is_minimized().unwrap_or(true))
        .filter(|w| w.pid().map(|p| p != me).unwrap_or(false))
        .filter_map(|w| {
            let title = w.title().ok()?;
            let (width, height) = (w.width().ok()?, w.height().ok()?);
            if title.trim().is_empty() || width < 50 || height < 50 {
                return None;
            }
            // 앱 이름이 전체 경로로 오는 경우(스토어 앱 등)는 파일 이름만 보여준다
            let app = w.app_name().unwrap_or_default();
            let app = app.rsplit(['\\', '/']).next().unwrap_or(&app).trim_end_matches(".exe").to_string();
            Some(WindowInfo { id: w.id().ok()?, app, title, width, height })
        })
        .collect())
}

pub fn capture_window(app: &AppHandle, id: u32) -> AppResult<()> {
    let hidden = hide_own_windows(app);
    let r = (|| -> AppResult<()> {
        let w = xcap::Window::all()
            .map_err(|e| AppError::new("CAPTURE_FAILED", e.to_string()))?
            .into_iter()
            .find(|w| w.id().ok() == Some(id))
            .ok_or_else(|| AppError::not_found("창이 닫혔거나 찾을 수 없습니다"))?;
        let img = w.capture_image().map_err(|e| AppError::new("CAPTURE_FAILED", format!("창 캡처 실패: {e}")))?;
        store(app, to_png(&img)?);
        Ok(())
    })();
    restore_windows(app, &hidden, &["editor", "overlay"]);
    r?;
    open_editor(app)
}

/// 영역 선택 결과(이미지 픽셀 좌표)로 잘라 편집 창으로 넘긴다.
pub fn crop(app: &AppHandle, x: u32, y: u32, w: u32, h: u32) -> AppResult<()> {
    let png = get(app).ok_or_else(|| AppError::not_found("캡처한 이미지가 없습니다"))?;
    let img = image::load_from_memory(&png).map_err(|e| AppError::new("CAPTURE_DECODE", e.to_string()))?.to_rgba8();
    let (iw, ih) = img.dimensions();
    let (x, y) = (x.min(iw.saturating_sub(1)), y.min(ih.saturating_sub(1)));
    let (w, h) = (w.clamp(1, iw - x), h.clamp(1, ih - y));
    let cropped = image::imageops::crop_imm(&img, x, y, w, h).to_image();
    store(app, to_png(&cropped)?);
    if let Some(o) = app.get_webview_window("overlay") {
        let _ = o.close();
    }
    open_editor(app)
}

pub fn get(app: &AppHandle) -> Option<Vec<u8>> {
    app.try_state::<CaptureState>().and_then(|s| s.lock().unwrap_or_else(|e| e.into_inner()).png.clone())
}

/// gen을 주면 그 캡처일 때만 지운다(그 사이 새 캡처가 들어왔으면 유지).
pub fn clear(app: &AppHandle, gen: Option<u64>) {
    if let Some(s) = app.try_state::<CaptureState>() {
        let mut st = s.lock().unwrap_or_else(|e| e.into_inner());
        if gen.map(|g| g == st.gen).unwrap_or(true) {
            st.png = None;
        }
    }
}

fn open_overlay(app: &AppHandle, m: &xcap::Monitor) -> AppResult<()> {
    if let Some(o) = app.get_webview_window("overlay") {
        let _ = o.close();
    }
    let w = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("index.html#/overlay".into()))
        .title("영역 선택")
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .visible(false)
        .build()?;
    let (x, y) = (m.x().unwrap_or(0), m.y().unwrap_or(0));
    let (mw, mh) = (m.width().unwrap_or(1920), m.height().unwrap_or(1080));
    let _ = w.set_position(PhysicalPosition::new(x, y));
    let _ = w.set_size(PhysicalSize::new(mw, mh));
    let _ = w.show();
    let _ = w.set_focus();
    Ok(())
}

/// 편집기 창이 사라질 때(lib.rs의 창 이벤트) 호출: 그 편집기가 보던 캡처만 메모리에서 지운다.
pub fn on_editor_closed(app: &AppHandle) {
    let gen = app.try_state::<CaptureState>().map(|s| s.lock().unwrap_or_else(|e| e.into_inner()).editor_gen);
    clear(app, gen);
}

fn mark_editor_gen(app: &AppHandle) {
    if let Some(s) = app.try_state::<CaptureState>() {
        let mut st = s.lock().unwrap_or_else(|e| e.into_inner());
        st.editor_gen = st.gen;
    }
}

pub fn open_editor(app: &AppHandle) -> AppResult<()> {
    mark_editor_gen(app);
    // 이미 열린 편집기가 있으면 닫지 않고 새 캡처를 다시 불러오게 한다
    if let Some(e) = app.get_webview_window("editor") {
        use tauri::Emitter;
        let _ = e.emit("capture://updated", generation(app));
        let _ = e.unminimize();
        let _ = e.show();
        let _ = e.set_focus();
        return Ok(());
    }
    let w = WebviewWindowBuilder::new(app, "editor", WebviewUrl::App("index.html#/editor".into()))
        .title("화면 캡처 — AutoDesk")
        .inner_size(1320.0, 820.0)
        .min_inner_size(760.0, 520.0)
        .visible(true)
        .build()?;
    let _ = w.set_focus();
    Ok(())
}

pub fn decode_b64(b64: &str) -> AppResult<Vec<u8>> {
    let raw = b64.split_once(',').map(|(_, d)| d).unwrap_or(b64);
    base64::engine::general_purpose::STANDARD.decode(raw).map_err(|e| AppError::invalid(format!("이미지 데이터 오류: {e}")))
}

/// 편집 결과(PNG base64)를 시스템 클립보드에 이미지로 넣는다.
pub fn copy_to_clipboard(app: &AppHandle, b64: &str) -> AppResult<()> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    let png = decode_b64(b64)?;
    let img = image::load_from_memory(&png).map_err(|e| AppError::new("CAPTURE_DECODE", e.to_string()))?.to_rgba8();
    let (w, h) = img.dimensions();
    let tauri_img = tauri::image::Image::new_owned(img.into_raw(), w, h);
    app.clipboard().write_image(&tauri_img).map_err(|e| AppError::new("CLIPBOARD_FAILED", format!("클립보드 복사 실패: {e}")))
}
