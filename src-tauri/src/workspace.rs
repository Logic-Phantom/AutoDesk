//! 업무 모드: 업무 시작 / 자리 비움 / 퇴근.
//!
//! - 업무 시작: 등록한 프로그램·문서·폴더·웹사이트를 한 번에 연다.
//! - 자리 비움: 화면 잠금 또는 모니터 끄기(선택), 자동화 일시정지(선택).
//! - 퇴근: 등록한 정리 명령(예: Tomcat shutdown.bat)을 실행하고, 등록한 프로그램에 '정상 종료'를 요청한다.
//!   저장하지 않은 문서가 있으면 각 프로그램이 직접 저장 여부를 묻는다(강제 저장/강제 종료는 기본 OFF).
//! 모든 대상은 사용자가 직접 등록한 것만 실행·종료한다. 시스템 프로세스와 AutoDesk 자신은 닫지 않는다.

use crate::core::Core;
use crate::db;
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LaunchItem {
    /// app | url | file
    pub kind: String,
    pub target: String,
    pub args: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CommandItem {
    pub label: String,
    /// 실행 파일 또는 .bat/.cmd/.ps1 경로
    pub path: String,
    pub args: String,
    pub cwd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CloseItem {
    /// 실행 파일 이름(예: chrome.exe, EXCEL.EXE). macOS는 앱 이름(예: Google Chrome)
    pub process: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkspaceConfig {
    pub dock_enabled: bool,
    pub start_items: Vec<LaunchItem>,
    pub start_resume_automation: bool,
    /// lock | display_off | none
    pub away_action: String,
    pub away_pause_automation: bool,
    pub end_commands: Vec<CommandItem>,
    pub end_close: Vec<CloseItem>,
    pub end_close_timeout_sec: u64,
    pub end_force_kill: bool,
    /// none | lock | shutdown
    pub end_after: String,
    pub end_pause_automation: bool,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            dock_enabled: true,
            start_items: vec![],
            start_resume_automation: true,
            away_action: "lock".into(),
            away_pause_automation: false,
            end_commands: vec![],
            end_close: vec![],
            end_close_timeout_sec: 20,
            end_force_kill: false,
            end_after: "none".into(),
            end_pause_automation: false,
        }
    }
}

/// 절대 닫지 않는 프로세스(소문자)
const PROTECTED: &[&str] = &[
    "explorer.exe", "autodesk.exe", "svchost.exe", "csrss.exe", "winlogon.exe", "dwm.exe", "lsass.exe", "services.exe",
    "smss.exe", "wininit.exe", "taskmgr.exe", "system", "finder", "dock", "loginwindow", "windowserver",
];

pub fn is_protected(process: &str) -> bool {
    PROTECTED.contains(&process.trim().to_lowercase().as_str())
}

pub fn load(core: &Core) -> WorkspaceConfig {
    db::get_setting_or(&core.db(), "workspace", WorkspaceConfig::default())
}

pub fn validate(mut c: WorkspaceConfig) -> AppResult<WorkspaceConfig> {
    for it in &c.start_items {
        match it.kind.as_str() {
            "url" => {
                if !(it.target.starts_with("https://") || it.target.starts_with("http://")) {
                    return Err(AppError::invalid(format!("웹 주소는 http:// 또는 https://로 시작해야 합니다: {}", it.target)));
                }
            }
            "app" | "file" => {
                if it.target.trim().is_empty() {
                    return Err(AppError::invalid("경로가 비어 있습니다"));
                }
            }
            _ => return Err(AppError::invalid("알 수 없는 항목 종류입니다")),
        }
    }
    for c in &c.end_close {
        if is_protected(&c.process) {
            return Err(AppError::invalid(format!("{}는 시스템/AutoDesk 프로세스라 닫을 수 없습니다", c.process)));
        }
    }
    if !["lock", "display_off", "none"].contains(&c.away_action.as_str()) {
        return Err(AppError::invalid("자리 비움 동작이 올바르지 않습니다"));
    }
    if !["none", "lock", "shutdown"].contains(&c.end_after.as_str()) {
        return Err(AppError::invalid("퇴근 후 동작이 올바르지 않습니다"));
    }
    c.end_close_timeout_sec = c.end_close_timeout_sec.clamp(5, 120);
    Ok(c)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepResult {
    pub label: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModeResult {
    pub mode: String,
    pub steps: Vec<StepResult>,
}

fn step(label: impl Into<String>, r: Result<String, String>) -> StepResult {
    match r {
        Ok(d) => StepResult { label: label.into(), ok: true, detail: d },
        Err(d) => StepResult { label: label.into(), ok: false, detail: d },
    }
}

fn hidden(cmd: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

fn split_args(s: &str) -> Vec<String> {
    // 큰따옴표로 묶은 인자를 지원하는 단순 분리
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in s.chars() {
        match ch {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

// ---------------- 업무 시작 ----------------

pub fn run_start(core: &Arc<Core>) -> ModeResult {
    use tauri_plugin_opener::OpenerExt;
    let cfg = load(core);
    let mut steps = Vec::new();
    for it in &cfg.start_items {
        let label = if it.label.is_empty() { it.target.clone() } else { it.label.clone() };
        let r = match it.kind.as_str() {
            "url" => core.app.opener().open_url(&it.target, None::<&str>).map(|_| "열었습니다".to_string()).map_err(|e| e.to_string()),
            "file" => {
                if Path::new(&it.target).exists() {
                    core.app.opener().open_path(&it.target, None::<&str>).map(|_| "열었습니다".to_string()).map_err(|e| e.to_string())
                } else {
                    Err("파일/폴더를 찾을 수 없습니다".into())
                }
            }
            _ => {
                let p = Path::new(&it.target);
                if !p.exists() {
                    Err("프로그램을 찾을 수 없습니다".into())
                } else if it.args.trim().is_empty() {
                    core.app.opener().open_path(&it.target, None::<&str>).map(|_| "실행했습니다".to_string()).map_err(|e| e.to_string())
                } else {
                    let mut cmd = std::process::Command::new(p);
                    cmd.args(split_args(&it.args));
                    if let Some(dir) = p.parent() {
                        cmd.current_dir(dir);
                    }
                    cmd.spawn().map(|_| "실행했습니다".to_string()).map_err(|e| e.to_string())
                }
            }
        };
        steps.push(step(label, r));
        std::thread::sleep(Duration::from_millis(250)); // 한꺼번에 몰리지 않게
    }
    if cfg.start_resume_automation && !core.runtime().master {
        steps.push(step("전체 자동화 켜기", core.set_master(true).map(|_| "켰습니다".into()).map_err(|e| e.message)));
    }
    if steps.is_empty() {
        steps.push(StepResult { label: "업무 시작".into(), ok: false, detail: "등록한 항목이 없습니다. 설정에서 추가하세요".into() });
    }
    finish(core, "start", steps)
}

// ---------------- 자리 비움 ----------------

pub fn run_away(core: &Arc<Core>) -> ModeResult {
    let cfg = load(core);
    let mut steps = Vec::new();
    if cfg.away_pause_automation && core.runtime().master {
        steps.push(step("자동화 일시정지", core.set_master(false).map(|_| "멈췄습니다".into()).map_err(|e| e.message)));
    }
    match cfg.away_action.as_str() {
        "lock" => steps.push(step("화면 잠금", platform::lock_screen())),
        "display_off" => steps.push(step("모니터 끄기", platform::display_off())),
        _ => {}
    }
    if steps.is_empty() {
        steps.push(StepResult { label: "자리 비움".into(), ok: true, detail: "설정된 동작이 없습니다".into() });
    }
    finish(core, "away", steps)
}

// ---------------- 퇴근 ----------------

/// 퇴근 모드에서 실제로 닫히게 될 창(미리보기용).
pub fn preview_end(core: &Core) -> Vec<platform::AppWindow> {
    let cfg = load(core);
    let names: Vec<String> = cfg.end_close.iter().map(|c| c.process.to_lowercase()).collect();
    platform::list_windows().into_iter().filter(|w| names.contains(&w.process.to_lowercase())).collect()
}

pub fn run_end(core: &Arc<Core>) -> ModeResult {
    let cfg = load(core);
    let mut steps = Vec::new();
    // 1) 정리 명령(서버 종료 스크립트 등)을 먼저 순서대로
    for c in &cfg.end_commands {
        let label = if c.label.is_empty() { c.path.clone() } else { c.label.clone() };
        steps.push(step(label, run_command(c)));
    }
    // 2) 프로그램에 정상 종료 요청
    for c in &cfg.end_close {
        if is_protected(&c.process) {
            continue;
        }
        let label = if c.label.is_empty() { c.process.clone() } else { c.label.clone() };
        steps.push(step(label, close_process(&c.process, cfg.end_close_timeout_sec, cfg.end_force_kill)));
    }
    if cfg.end_pause_automation && core.runtime().master {
        steps.push(step("자동화 일시정지", core.set_master(false).map(|_| "멈췄습니다".into()).map_err(|e| e.message)));
    }
    let all_closed = steps.iter().all(|s| s.ok);
    match cfg.end_after.as_str() {
        "lock" => steps.push(step("화면 잠금", platform::lock_screen())),
        "shutdown" if all_closed => steps.push(step("PC 종료 예약(60초 후)", platform::schedule_shutdown())),
        "shutdown" => steps.push(StepResult {
            label: "PC 종료".into(),
            ok: false,
            detail: "닫히지 않은 프로그램이 있어 PC 종료를 건너뛰었습니다(저장 여부를 확인하세요)".into(),
        }),
        _ => {}
    }
    if steps.is_empty() {
        steps.push(StepResult { label: "퇴근".into(), ok: false, detail: "등록한 항목이 없습니다. 설정에서 추가하세요".into() });
    }
    finish(core, "end", steps)
}

fn run_command(c: &CommandItem) -> Result<String, String> {
    let p = Path::new(&c.path);
    if !p.is_file() {
        return Err("파일을 찾을 수 없습니다".into());
    }
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let mut cmd = match ext.as_str() {
        "bat" | "cmd" => {
            let mut x = std::process::Command::new("cmd");
            x.arg("/C").arg(p);
            x
        }
        "ps1" => {
            let mut x = std::process::Command::new("powershell.exe");
            x.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]).arg(p);
            x
        }
        "sh" => {
            let mut x = std::process::Command::new("/bin/bash");
            x.arg(p);
            x
        }
        _ => std::process::Command::new(p),
    };
    cmd.args(split_args(&c.args));
    let dir = if c.cwd.trim().is_empty() { p.parent().map(|d| d.to_path_buf()) } else { Some(c.cwd.clone().into()) };
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    hidden(&mut cmd).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    let mut child = cmd.spawn().map_err(|e| format!("실행 실패: {e}"))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(s)) if s.success() => return Ok("완료".into()),
            Ok(Some(s)) => return Err(format!("종료 코드 {}", s.code().unwrap_or(-1))),
            Ok(None) if start.elapsed() > Duration::from_secs(90) => {
                return Ok("90초가 지나 다음 단계로 진행했습니다(명령은 계속 실행 중)".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(300)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn close_process(process: &str, timeout: u64, force: bool) -> Result<String, String> {
    let n = platform::request_close(process);
    if n == 0 {
        return Ok("실행 중이 아님".into());
    }
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(timeout) {
        std::thread::sleep(Duration::from_millis(500));
        if platform::count_windows(process) == 0 {
            return Ok(format!("정상 종료({n}개 창)"));
        }
    }
    if force {
        let k = platform::force_kill(process);
        return Ok(format!("응답이 없어 강제 종료({k}개 프로세스)"));
    }
    Err("아직 열려 있습니다 — 저장 확인 창이 떠 있는지 확인하세요".into())
}

fn finish(core: &Arc<Core>, mode: &str, steps: Vec<StepResult>) -> ModeResult {
    let ok = steps.iter().filter(|s| s.ok).count();
    let label = match mode {
        "start" => "업무 시작",
        "away" => "자리 비움",
        _ => "퇴근",
    };
    let result = if ok == steps.len() { "completed" } else { "failed" };
    let _ = db::add_activity(&core.db(), "workspace", &format!("workspace.{mode}"), result, None,
        &format!("{label}: {ok}/{} 단계 완료", steps.len()), None, None);
    core.notify_changed();
    ModeResult { mode: mode.into(), steps }
}

// ---------------- OS 어댑터 ----------------

pub mod platform {
    use serde::Serialize;

    #[derive(Debug, Clone, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AppWindow {
        pub process: String,
        pub title: String,
        pub pid: u32,
    }

    #[cfg(windows)]
    mod win {
        use super::AppWindow;
        use windows::core::{BOOL, PWSTR};
        use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WPARAM};
        use windows::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, TerminateProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
            PROCESS_TERMINATE,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            EnumWindows, GetWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW,
            GW_OWNER, WM_CLOSE,
        };

        fn process_name(pid: u32) -> Option<String> {
            unsafe {
                let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
                let mut buf = [0u16; 1024];
                let mut len = buf.len() as u32;
                let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
                let _ = CloseHandle(h);
                r.ok()?;
                let full = String::from_utf16_lossy(&buf[..len as usize]);
                full.rsplit(['\\', '/']).next().map(|s| s.to_string())
            }
        }

        pub struct Win {
            pub hwnd: HWND,
            pub info: AppWindow,
        }

        unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let out = &mut *(lparam.0 as *mut Vec<Win>);
            if !IsWindowVisible(hwnd).as_bool() {
                return BOOL(1);
            }
            // 소유자가 있는 창(대화상자 등)은 제외하고 최상위 앱 창만
            if GetWindow(hwnd, GW_OWNER).map(|o| !o.is_invalid()).unwrap_or(false) {
                return BOOL(1);
            }
            let len = GetWindowTextLengthW(hwnd);
            if len == 0 {
                return BOOL(1);
            }
            let mut buf = vec![0u16; len as usize + 1];
            let n = GetWindowTextW(hwnd, &mut buf);
            let title = String::from_utf16_lossy(&buf[..n as usize]);
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if let Some(process) = process_name(pid) {
                out.push(Win { hwnd, info: AppWindow { process, title, pid } });
            }
            BOOL(1)
        }

        pub fn windows() -> Vec<Win> {
            let mut v: Vec<Win> = Vec::new();
            unsafe {
                let _ = EnumWindows(Some(collect), LPARAM(&mut v as *mut Vec<Win> as isize));
            }
            let me = std::process::id();
            v.into_iter().filter(|w| w.info.pid != me).collect()
        }

        pub fn post_close(hwnd: HWND) {
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }

        pub fn terminate(pid: u32) -> bool {
            unsafe {
                match OpenProcess(PROCESS_TERMINATE, false, pid) {
                    Ok(h) => {
                        let ok = TerminateProcess(h, 1).is_ok();
                        let _ = CloseHandle(h);
                        ok
                    }
                    Err(_) => false,
                }
            }
        }
    }

    pub fn list_windows() -> Vec<AppWindow> {
        #[cfg(windows)]
        {
            win::windows().into_iter().map(|w| w.info).collect()
        }
        #[cfg(not(windows))]
        {
            xcap::Window::all()
                .unwrap_or_default()
                .into_iter()
                .filter(|w| !w.is_minimized().unwrap_or(false))
                .filter_map(|w| {
                    Some(AppWindow { process: w.app_name().ok()?, title: w.title().unwrap_or_default(), pid: w.pid().unwrap_or(0) })
                })
                .filter(|w| w.pid != std::process::id() && !w.title.is_empty())
                .collect()
        }
    }

    pub fn count_windows(process: &str) -> usize {
        let p = process.to_lowercase();
        list_windows().into_iter().filter(|w| w.process.to_lowercase() == p).count()
    }

    /// 해당 프로세스의 최상위 창에 닫기 요청(WM_CLOSE / macOS quit). 요청한 창 수를 돌려준다.
    pub fn request_close(process: &str) -> usize {
        if super::is_protected(process) {
            return 0;
        }
        #[cfg(windows)]
        {
            let p = process.to_lowercase();
            let wins: Vec<_> = win::windows().into_iter().filter(|w| w.info.process.to_lowercase() == p).collect();
            for w in &wins {
                win::post_close(w.hwnd);
            }
            wins.len()
        }
        #[cfg(target_os = "macos")]
        {
            let n = count_windows(process);
            if n > 0 {
                let script = format!("tell application \"{}\" to quit", process.replace('"', ""));
                let _ = std::process::Command::new("osascript").args(["-e", &script]).status();
            }
            n
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            let _ = process;
            0
        }
    }

    pub fn force_kill(process: &str) -> usize {
        if super::is_protected(process) {
            return 0;
        }
        #[cfg(windows)]
        {
            let p = process.to_lowercase();
            let mut pids: Vec<u32> = win::windows().into_iter().filter(|w| w.info.process.to_lowercase() == p).map(|w| w.info.pid).collect();
            pids.sort();
            pids.dedup();
            pids.into_iter().filter(|pid| win::terminate(*pid)).count()
        }
        #[cfg(not(windows))]
        {
            let _ = std::process::Command::new("pkill").args(["-x", process]).status();
            1
        }
    }

    pub fn lock_screen() -> Result<String, String> {
        #[cfg(windows)]
        unsafe {
            windows::Win32::System::Shutdown::LockWorkStation().map(|_| "잠갔습니다".to_string()).map_err(|e| e.to_string())
        }
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("pmset").arg("displaysleepnow").status().map(|_| "화면을 껐습니다(암호 요구 설정 시 잠김)".into()).map_err(|e| e.to_string())
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            Err("지원하지 않는 OS".into())
        }
    }

    pub fn display_off() -> Result<String, String> {
        #[cfg(windows)]
        unsafe {
            use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, HWND_BROADCAST, SC_MONITORPOWER, WM_SYSCOMMAND};
            let _ = PostMessageW(Some(HWND_BROADCAST), WM_SYSCOMMAND, WPARAM(SC_MONITORPOWER as usize), LPARAM(2));
            let _: Option<HWND> = None;
            Ok("모니터를 껐습니다".into())
        }
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("pmset").arg("displaysleepnow").status().map(|_| "모니터를 껐습니다".into()).map_err(|e| e.to_string())
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            Err("지원하지 않는 OS".into())
        }
    }

    /// 60초 뒤 종료 예약(취소: shutdown /a 또는 앱 화면의 취소 버튼)
    pub fn schedule_shutdown() -> Result<String, String> {
        #[cfg(windows)]
        {
            let mut cmd = std::process::Command::new("shutdown");
            cmd.args(["/s", "/t", "60", "/c", "AutoDesk 퇴근 모드: 60초 후 PC를 종료합니다. 취소하려면 AutoDesk에서 '종료 취소'를 누르세요."]);
            super::hidden(&mut cmd).status().map(|_| "60초 후 종료".into()).map_err(|e| e.to_string())
        }
        #[cfg(not(windows))]
        {
            Err("macOS에서는 PC 자동 종료를 지원하지 않습니다".into())
        }
    }

    pub fn cancel_shutdown() -> Result<String, String> {
        #[cfg(windows)]
        {
            let mut cmd = std::process::Command::new("shutdown");
            cmd.arg("/a");
            super::hidden(&mut cmd).status().map(|_| "종료 예약을 취소했습니다".into()).map_err(|e| e.to_string())
        }
        #[cfg(not(windows))]
        {
            Err("지원하지 않음".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_split_with_quotes() {
        assert_eq!(split_args(r#"--a "C:\Program Files\x" -b"#), vec!["--a", r"C:\Program Files\x", "-b"]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn protected_processes_rejected() {
        assert!(is_protected("explorer.exe"));
        assert!(is_protected("AutoDesk.exe"));
        assert!(!is_protected("chrome.exe"));
        let c = WorkspaceConfig { end_close: vec![CloseItem { process: "explorer.exe".into(), label: String::new() }], ..Default::default() };
        assert!(validate(c).is_err());
    }

    #[test]
    fn url_validation() {
        let bad = WorkspaceConfig {
            start_items: vec![LaunchItem { kind: "url".into(), target: "javascript:alert(1)".into(), ..Default::default() }],
            ..Default::default()
        };
        assert!(validate(bad).is_err());
        let ok = WorkspaceConfig {
            start_items: vec![LaunchItem { kind: "url".into(), target: "https://github.com".into(), ..Default::default() }],
            ..Default::default()
        };
        assert!(validate(ok).is_ok());
    }
}
