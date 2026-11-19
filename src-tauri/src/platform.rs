//! OS 차이를 격리하는 어댑터: 로그인 자동 실행, 설치 위치, 앱 소유 경로, 제거 도우미.
//! 경로는 Tauri path API로 계산하고, 하드코딩된 사용자 경로를 추측해서 지우지 않는다.

use crate::error::{AppError, AppResult};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

/// autostart 플러그인이 등록에 쓰는 이름. Windows: HKCU\...\Run 값 이름, macOS: ~/Library/LaunchAgents/<이름>.plist
pub const AUTOSTART_NAME: &str = "AutoDesk";
pub const AUTOSTART_ARG: &str = "--autostart";
pub const QUIT_ARG: &str = "--quit";

pub fn autostart_is_enabled(app: &AppHandle) -> (bool, Option<String>) {
    match app.autolaunch().is_enabled() {
        Ok(v) => (v, None),
        Err(e) => (false, Some(format!("자동 실행 상태 확인 실패: {e}"))),
    }
}

/// OS 등록을 바꾼 뒤 실제 상태를 다시 읽어 확인한다. 등록이 실패하면 성공으로 보고하지 않는다.
pub fn set_autostart(app: &AppHandle, enabled: bool) -> AppResult<bool> {
    let m = app.autolaunch();
    let r = if enabled { m.enable() } else { m.disable() };
    if let Err(e) = r {
        // 이미 해제된 상태에서 disable하면 오류가 날 수 있다 → 실제 상태로 판정
        let actual = m.is_enabled().unwrap_or(!enabled);
        if actual != enabled {
            return Err(AppError::new("AUTOSTART_FAILED", format!("로그인 시 자동 실행 설정 실패: {e}")));
        }
    }
    let actual = m.is_enabled().map_err(|e| AppError::new("AUTOSTART_FAILED", e.to_string()))?;
    if actual != enabled {
        return Err(AppError::new("AUTOSTART_FAILED", "OS에 반영되지 않았습니다"));
    }
    Ok(actual)
}

/// 개발 빌드(target/debug 등)에서 실행 중인지. 개발 중에는 실행 파일을 지우지 않는다.
pub fn is_dev_build() -> bool {
    cfg!(debug_assertions)
        || std::env::current_exe()
            .map(|p| p.components().any(|c| c.as_os_str() == "target"))
            .unwrap_or(true)
}

/// 앱이 소유한 데이터 폴더 목록(존재하는 것만). 모두 앱 식별자를 포함하는 경로다.
pub fn owned_data_dirs(app: &AppHandle) -> Vec<(String, PathBuf)> {
    let p = app.path();
    let id = app.config().identifier.clone();
    let mut v: Vec<(String, PathBuf)> = Vec::new();
    let mut push = |label: &str, path: Option<PathBuf>| {
        if let Some(path) = path {
            if path.to_string_lossy().contains(&id) && !v.iter().any(|(_, x)| x == &path) {
                v.push((label.to_string(), path));
            }
        }
    };
    push("설정·DB·백업·로그", p.app_data_dir().ok());
    push("로컬 데이터(WebView 캐시)", p.app_local_data_dir().ok());
    push("캐시", p.app_cache_dir().ok());
    push("로그(OS 기본 위치)", p.app_log_dir().ok());
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = p.home_dir() {
            push("WebKit 데이터", Some(home.join("Library/WebKit").join(&id)));
            push("창 상태", Some(home.join("Library/Saved Application State").join(format!("{id}.savedState"))));
        }
    }
    v.into_iter().filter(|(_, p)| p.exists()).collect()
}

/// 설치된 앱 위치. Windows: 실행 파일 폴더의 uninstall.exe, macOS: AutoDesk.app 번들.
pub fn install_location() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    #[cfg(windows)]
    {
        let uninstaller = exe.parent()?.join("uninstall.exe");
        uninstaller.is_file().then_some(uninstaller)
    }
    #[cfg(target_os = "macos")]
    {
        // .../AutoDesk.app/Contents/MacOS/AutoDesk
        let bundle = exe.parent()?.parent()?.parent()?.to_path_buf();
        (bundle.extension().map(|e| e == "app").unwrap_or(false)
            && bundle.file_name().map(|n| n == "AutoDesk.app").unwrap_or(false))
        .then_some(bundle)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = exe;
        None
    }
}
