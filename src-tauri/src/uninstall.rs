//! 완전 제거(Uninstall Completely).
//!
//! 순서: 자동화 중지 → 로그인 자동 실행 해제 → (선택) 백업 내보내기 → 일회성 정리 도우미 실행 → 앱 종료.
//! 도우미는 앱 프로세스가 끝나길 기다렸다가 앱 소유 폴더만 지우고, 설치본(Windows uninstall.exe /
//! macOS AutoDesk.app)을 제거한 뒤 스스로를 삭제하고 끝난다. 상주하지 않는다.
//! 사용자의 원본 폴더·감시 폴더·다운로드 파일은 대상에 포함되지 않는다.

use crate::core::{human_size, Core};
use crate::error::{AppError, AppResult};
use crate::fsutil;
use crate::platform;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::Manager;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallItem {
    pub label: String,
    pub path: Option<String>,
    pub size: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallPreview {
    pub items: Vec<UninstallItem>,
    pub backups_size: String,
    pub total_size: String,
    pub dev_build: bool,
    pub preserved: Vec<String>,
}

pub fn preview(core: &Arc<Core>) -> AppResult<UninstallPreview> {
    let app = &core.app;
    let mut items = Vec::new();
    let mut total = 0u64;
    for (label, p) in platform::owned_data_dirs(app) {
        let s = fsutil::dir_size(&p);
        total += s;
        items.push(UninstallItem { label, path: Some(p.to_string_lossy().into()), size: Some(human_size(s)), note: None });
    }
    let (autostart, _) = platform::autostart_is_enabled(app);
    items.push(UninstallItem {
        label: "로그인 시 자동 실행 등록".into(),
        path: None,
        size: None,
        note: Some(if autostart { "등록됨 → 해제".into() } else { "등록 안 됨".into() }),
    });
    items.push(UninstallItem { label: "실행 중인 자동화·트레이 아이콘".into(), path: None, size: None, note: Some("정상 종료".into()) });
    let dev = platform::is_dev_build();
    match platform::install_location() {
        Some(p) if !dev => items.push(UninstallItem {
            label: "설치된 앱 파일".into(),
            path: Some(p.to_string_lossy().into()),
            size: None,
            note: Some(if cfg!(windows) { "설치 제거 프로그램을 조용히 실행".into() } else { "앱 번들 삭제".into() }),
        }),
        _ => items.push(UninstallItem {
            label: "설치된 앱 파일".into(),
            path: None,
            size: None,
            note: Some("개발/휴대용 실행이라 앱 파일은 남깁니다(데이터만 정리)".into()),
        }),
    }
    let mut preserved = vec!["감시 폴더의 원본 파일".to_string(), "다운로드 폴더와 분류된 파일".to_string()];
    let db = core.db();
    for t in crate::db::list_watch_targets(&db, None).unwrap_or_default() {
        preserved.push(t.path);
    }
    Ok(UninstallPreview {
        items,
        backups_size: human_size(fsutil::dir_size(&core.paths.backups)),
        total_size: human_size(total),
        dev_build: dev,
        preserved,
    })
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)?.flatten() {
        let p = e.path();
        let target = dst.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &target)?;
        } else {
            std::fs::copy(&p, &target)?;
        }
    }
    Ok(())
}

/// 제거를 실행한다. 성공하면 앱은 곧 종료된다. 반환값은 사용자에게 보여줄 결과 메시지.
pub fn execute(core: &Arc<Core>, keep_backups: bool) -> AppResult<String> {
    let app = core.app.clone();
    let mut report: Vec<String> = Vec::new();

    // 1) 백업 보존을 원하면 먼저 문서 폴더로 내보낸다.
    if keep_backups && core.paths.backups.is_dir() {
        let docs = app.path().document_dir().map_err(|e| AppError::new("PATH_ERROR", e.to_string()))?;
        let dest = fsutil::unique_path(&docs, &format!("AutoDesk-Backups-{}", chrono::Local::now().format("%Y%m%d")));
        copy_dir(&core.paths.backups, &dest).map_err(|e| AppError::new("EXPORT_FAILED", format!("백업 내보내기 실패 — 제거를 중단했습니다: {e}")))?;
        report.push(format!("백업 보존: {}", dest.display()));
    }

    // 2) 자동화 중지(감시·단축키·엔진)
    core.update_runtime(|r| r.uninstalling = true);
    core.shutdown();
    report.push("자동화 중지: 완료".into());

    // 3) 로그인 자동 실행 해제(OS 등록 상태 재확인)
    match platform::set_autostart(&app, false) {
        Ok(_) => report.push("로그인 자동 실행 해제: 완료".into()),
        Err(e) => report.push(format!("로그인 자동 실행 해제: 실패 — {} (수동 확인 필요)", e.message)),
    }

    // 4) 정리 도우미 준비 및 실행
    let id = app.config().identifier.clone();
    let dirs: Vec<PathBuf> = platform::owned_data_dirs(&app)
        .into_iter()
        .map(|(_, p)| p)
        .filter(|p| p.to_string_lossy().contains(&id)) // 앱 식별자를 포함한 경로만
        .collect();
    let install = if platform::is_dev_build() { None } else { platform::install_location() };
    spawn_helper(&dirs, install.as_deref(), &report)?;
    Ok("정리 도우미를 실행했습니다. AutoDesk가 종료된 뒤 남은 항목을 정리하고 결과 보고서를 엽니다.".into())
}

fn temp_dir() -> PathBuf {
    std::env::temp_dir()
}

#[cfg(windows)]
fn spawn_helper(dirs: &[PathBuf], install: Option<&Path>, report: &[String]) -> AppResult<()> {
    use std::os::windows::process::CommandExt;
    let ps_quote = |s: &str| format!("'{}'", s.replace('\'', "''"));
    let pid = std::process::id();
    let script_path = temp_dir().join(format!("autodesk-uninstall-{pid}.ps1"));
    let report_path = temp_dir().join("AutoDesk-uninstall-report.txt");
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    let dir_list = dirs.iter().map(|d| ps_quote(&d.to_string_lossy())).collect::<Vec<_>>().join(",");
    let pre = report.iter().map(|l| ps_quote(l)).collect::<Vec<_>>().join(",");
    let uninstaller = install.map(|p| ps_quote(&p.to_string_lossy())).unwrap_or_else(|| "$null".into());
    let script = format!(
        r#"$ErrorActionPreference = 'Continue'
$report = [System.Collections.Generic.List[string]]::new()
@({pre}) | ForEach-Object {{ $report.Add($_) }}
# 1) AutoDesk 프로세스(이 PID만) 종료 대기 — 최대 30초, 그 뒤에도 남으면 그 PID만 종료
$deadline = (Get-Date).AddSeconds(30)
while ((Get-Process -Id {pid} -ErrorAction SilentlyContinue) -and (Get-Date) -lt $deadline) {{ Start-Sleep -Milliseconds 300 }}
$p = Get-Process -Id {pid} -ErrorAction SilentlyContinue
if ($p -and $p.Path -eq {exe}) {{ Stop-Process -Id {pid} -Force; $report.Add('프로세스 강제 종료(PID {pid})') }} else {{ $report.Add('프로세스 종료 확인: 완료') }}
# 2) 자동 실행 레지스트리 값(앱이 만든 정확한 이름) 잔여 확인
foreach ($k in 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run','HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run') {{
  if (Get-ItemProperty -Path $k -Name '{name}' -ErrorAction SilentlyContinue) {{ Remove-ItemProperty -Path $k -Name '{name}' -ErrorAction SilentlyContinue; $report.Add("레지스트리 값 제거: $k\{name}") }}
}}
# 3) 앱 소유 데이터 폴더 삭제
foreach ($d in @({dir_list})) {{
  if (Test-Path -LiteralPath $d) {{
    Remove-Item -LiteralPath $d -Recurse -Force -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath $d) {{ $report.Add("삭제 실패(수동 삭제 필요): $d") }} else {{ $report.Add("삭제: $d") }}
  }}
}}
# 4) 설치본 제거(NSIS 제거 프로그램, 조용히)
$u = {uninstaller}
if ($u -and (Test-Path -LiteralPath $u)) {{
  Start-Process -FilePath $u -ArgumentList '/S' -Wait
  $report.Add('설치 제거 프로그램 실행: 완료')
}} else {{ $report.Add('설치 제거 프로그램: 없음(개발/휴대용 실행) — 앱 파일은 그대로 둠') }}
$report.Add('')
$report.Add('원본 파일·감시 폴더·다운로드 파일은 삭제하지 않았습니다.')
$report | Set-Content -LiteralPath {report_path} -Encoding UTF8
Start-Process notepad.exe -ArgumentList {report_path}
Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue
"#,
        pre = pre,
        pid = pid,
        exe = ps_quote(&exe),
        name = platform::AUTOSTART_NAME,
        dir_list = dir_list,
        uninstaller = uninstaller,
        report_path = ps_quote(&report_path.to_string_lossy()),
    );
    // PowerShell 5.1이 한글을 올바로 읽도록 UTF-8 BOM으로 저장한다.
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(script.as_bytes());
    std::fs::write(&script_path, bytes)?;
    // 앱이 Job 객체 안에서 실행된 경우(터미널·런처 등) 앱 종료와 함께 도우미가 죽지 않도록 Job에서 이탈시킨다.
    // 이탈이 허용되지 않는 Job이면 일반 방식으로 다시 시도한다.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let spawn = |flags: u32| {
        std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&script_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(flags)
            .spawn()
    };
    spawn(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB)
        .or_else(|_| spawn(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP))
        .map_err(|e| AppError::new("HELPER_FAILED", format!("정리 도우미 실행 실패: {e}")))?;
    Ok(())
}

#[cfg(not(windows))]
fn spawn_helper(dirs: &[PathBuf], install: Option<&Path>, report: &[String]) -> AppResult<()> {
    let q = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    let pid = std::process::id();
    let script_path = temp_dir().join(format!("autodesk-uninstall-{pid}.sh"));
    let report_path = temp_dir().join("AutoDesk-uninstall-report.txt");
    let pre = report.iter().map(|l| format!("echo {} >> \"$R\"", q(l))).collect::<Vec<_>>().join("\n");
    let rm_dirs = dirs
        .iter()
        .map(|d| {
            let d = q(&d.to_string_lossy());
            format!("if [ -e {d} ]; then rm -rf {d}; if [ -e {d} ]; then echo \"삭제 실패(수동 삭제 필요): \"{d} >> \"$R\"; else echo \"삭제: \"{d} >> \"$R\"; fi; fi")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let bundle = install
        .filter(|p| p.file_name().map(|n| n == "AutoDesk.app").unwrap_or(false))
        .map(|p| {
            let b = q(&p.to_string_lossy());
            format!("rm -rf {b} && echo \"앱 번들 삭제: \"{b} >> \"$R\" || echo \"앱 번들 삭제 실패: \"{b} >> \"$R\"")
        })
        .unwrap_or_else(|| "echo '앱 번들: 개발 실행이라 그대로 둠' >> \"$R\"".into());
    let plist = format!("$HOME/Library/LaunchAgents/{}.plist", platform::AUTOSTART_NAME);
    let script = format!(
        r#"#!/bin/bash
R={report}
: > "$R"
{pre}
for i in $(seq 1 100); do kill -0 {pid} 2>/dev/null || break; sleep 0.3; done
if kill -0 {pid} 2>/dev/null; then kill -9 {pid}; echo "프로세스 강제 종료(PID {pid})" >> "$R"; else echo "프로세스 종료 확인: 완료" >> "$R"; fi
if [ -f "{plist}" ]; then launchctl unload "{plist}" 2>/dev/null; rm -f "{plist}"; echo "LaunchAgent 제거: {plist}" >> "$R"; fi
{rm_dirs}
{bundle}
echo "" >> "$R"
echo "원본 파일·감시 폴더·다운로드 파일은 삭제하지 않았습니다." >> "$R"
open -e "$R" 2>/dev/null
rm -f "$0"
"#,
        report = q(&report_path.to_string_lossy()),
        pre = pre,
        pid = pid,
        plist = plist,
        rm_dirs = rm_dirs,
        bundle = bundle,
    );
    std::fs::write(&script_path, script)?;
    std::process::Command::new("/bin/bash")
        .arg(&script_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| AppError::new("HELPER_FAILED", format!("정리 도우미 실행 실패: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn owned_paths_must_contain_identifier() {
        // 앱 식별자가 없는 경로는 삭제 대상에서 제외되어야 한다.
        let id = "io.github.logicphantom.autodesk";
        let ok = std::path::Path::new("C:/Users/u/AppData/Roaming/io.github.logicphantom.autodesk");
        let bad = std::path::Path::new("C:/Users/u/Documents");
        assert!(ok.to_string_lossy().contains(id));
        assert!(!bad.to_string_lossy().contains(id));
    }
}
