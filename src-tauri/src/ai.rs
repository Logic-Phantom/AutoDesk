//! 선택 기능: 로컬에 설치된 Claude Code CLI(`claude -p`)로 텍스트를 다듬는다.
//! - API 키를 저장하지 않는다. 사용자가 이미 로그인한 Claude Code(본인 구독 플랜)를 그대로 호출한다.
//! - 기본 OFF. 사용자가 동의를 켜고, 전용 단축키를 누른 순간에만 클립보드 텍스트가 전송된다.
//! - 요청/응답 원문은 로그·DB에 남기지 않는다. 실패해도 로컬 기능에는 영향이 없다.

use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeStatus {
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
}

fn candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    let names: &[&str] = if cfg!(windows) { &["claude.exe", "claude.cmd"] } else { &["claude"] };
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for n in names {
                v.push(dir.join(n));
            }
        }
    }
    // GUI 앱은 PATH가 짧을 수 있으므로 공식 설치 위치도 확인한다.
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from) {
        for n in names {
            v.push(home.join(".local").join("bin").join(n));
            v.push(home.join(".claude").join("local").join(n));
        }
        if cfg!(windows) {
            if let Some(appdata) = std::env::var_os("APPDATA").map(PathBuf::from) {
                v.push(appdata.join("npm").join("claude.cmd"));
            }
        }
    }
    if cfg!(target_os = "macos") {
        v.push(PathBuf::from("/opt/homebrew/bin/claude"));
        v.push(PathBuf::from("/usr/local/bin/claude"));
    }
    v
}

pub fn find_claude() -> Option<PathBuf> {
    candidates().into_iter().find(|p| p.is_file())
}

fn base_command(bin: &Path) -> Command {
    let is_cmd = bin.extension().map(|e| e.eq_ignore_ascii_case("cmd")).unwrap_or(false);
    #[allow(unused_mut)]
    let mut cmd = if is_cmd {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(bin);
        c
    } else {
        Command::new(bin)
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: 콘솔 창 깜빡임 방지
    }
    cmd
}

pub fn status() -> ClaudeStatus {
    match find_claude() {
        None => ClaudeStatus { available: false, path: None, version: None },
        Some(p) => {
            let version = base_command(&p)
                .arg("--version")
                .stdin(Stdio::null())
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
            ClaudeStatus { available: version.is_some(), path: Some(p.to_string_lossy().to_string()), version }
        }
    }
}

const POLISH_INSTRUCTION: &str = "다음 <text> 안의 글을 다듬어라. 규칙: 맞춤법·띄어쓰기·오타만 고치고 의미·어투·줄바꿈·목록 구조는 유지한다. \
URL, 이메일, 파일 경로, 코드, 숫자는 그대로 둔다. 설명이나 머리말 없이 고친 글만 출력한다.";

/// 텍스트를 Claude Code로 다듬는다. 실패/시간 초과 시 오류를 돌려주고 원문은 유지된다.
pub fn polish(text: &str, model: &str, work_dir: &Path) -> AppResult<String> {
    let bin = find_claude().ok_or_else(|| AppError::new("AI_UNAVAILABLE", "Claude Code CLI를 찾지 못했습니다. 설치·로그인 후 다시 시도하세요"))?;
    if text.chars().count() > 20_000 {
        return Err(AppError::invalid("텍스트가 너무 깁니다(20,000자 이하만 다듬습니다)"));
    }
    std::fs::create_dir_all(work_dir)?;
    let mut cmd = base_command(&bin);
    // 프롬프트는 stdin으로 보낸다(셸 인용 문제 방지). 빈 작업 폴더에서 실행해 프로젝트 파일을 읽지 않게 한다.
    cmd.arg("-p").arg("--output-format").arg("text");
    let model = model.trim();
    if !model.is_empty() && model.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.') {
        cmd.arg("--model").arg(model);
    }
    cmd.current_dir(work_dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| AppError::new("AI_SPAWN_FAILED", format!("Claude Code 실행 실패: {e}")))?;
    {
        let mut stdin = child.stdin.take().ok_or_else(|| AppError::new("AI_SPAWN_FAILED", "입력 연결 실패"))?;
        let payload = format!("{POLISH_INSTRUCTION}\n\n<text>\n{text}\n</text>");
        stdin.write_all(payload.as_bytes())?;
    }
    let started = Instant::now();
    loop {
        match child.try_wait()? {
            Some(_) => break,
            None if started.elapsed() > Duration::from_secs(120) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::new("AI_TIMEOUT", "Claude Code 응답 시간이 초과되었습니다(120초)"));
            }
            None => std::thread::sleep(Duration::from_millis(200)),
        }
    }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        // stderr에는 사용자 텍스트가 포함되지 않지만, 길이만 제한해 보여준다.
        let err: String = String::from_utf8_lossy(&out.stderr).chars().take(200).collect();
        return Err(AppError::new("AI_FAILED", format!("Claude Code 오류: {err} (사용량 한도·로그인 상태를 확인하세요)")));
    }
    let result = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
    validate_output(text, &result)?;
    Ok(result)
}

/// 비어 있거나 원문보다 지나치게 길면(설명이 붙었을 가능성) 버린다.
pub fn validate_output(original: &str, result: &str) -> AppResult<()> {
    let (a, b) = (original.chars().count(), result.chars().count());
    if b == 0 || b > a * 2 + 50 {
        return Err(AppError::new("AI_BAD_OUTPUT", "AI 응답이 예상과 달라 적용하지 않았습니다(원문 유지)"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_validation() {
        assert!(validate_output("안녕 하세요", "안녕하세요").is_ok());
        assert!(validate_output("abc", "").is_err());
        assert!(validate_output("abc", &"x".repeat(500)).is_err());
    }
}
