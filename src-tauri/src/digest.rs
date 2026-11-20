//! Daily Work Digest: AutoDesk가 하루 동안 '실제로 처리한 일'만 모아 요약한다.
//! - 사용자의 다른 PC 활동이나 파일 내용은 보지 않는다(activity 테이블의 자체 기록만).
//! - 실제 건수만 표시하고 추정하지 않는다.
//! - 선택: 정해진 시각에 하루 한 번 알림(기본 OFF).

use crate::core::Core;
use crate::db;
use crate::error::{AppError, AppResult};
use chrono::{Local, NaiveDate, NaiveTime};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DigestSettings {
    pub notify_enabled: bool,
    /// "HH:MM" (로컬 시각)
    pub notify_time: String,
    /// 처리한 일이 하나도 없으면 알리지 않음
    pub skip_if_empty: bool,
}

impl Default for DigestSettings {
    fn default() -> Self {
        Self { notify_enabled: false, notify_time: "18:00".into(), skip_if_empty: true }
    }
}

pub fn validate(s: DigestSettings) -> AppResult<DigestSettings> {
    NaiveTime::parse_from_str(&s.notify_time, "%H:%M").map_err(|_| AppError::invalid("알림 시각은 HH:MM 형식이어야 합니다(예: 18:00)"))?;
    Ok(s)
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModuleCount {
    pub module_id: String,
    pub label: String,
    pub completed: i64,
    pub failed: i64,
    pub waiting: i64,
    pub skipped: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Digest {
    pub day: String,
    pub completed: i64,
    pub failed: i64,
    pub pending_approvals: i64,
    pub modules: Vec<ModuleCount>,
    /// 사람이 읽는 요약 문장들(건수가 있는 것만)
    pub highlights: Vec<String>,
    pub failures: Vec<db::ActivityRow>,
}

fn label(module: &str) -> &str {
    match module {
        "backup" => "AutoBackup",
        "downloads" => "Download Butler",
        "autopaste" => "AutoPaste",
        "workspace" => "업무 모드",
        "core" => "시스템",
        other => other,
    }
}

fn count(conn: &Connection, day: &str, where_sql: &str) -> AppResult<i64> {
    let sql = format!("SELECT COUNT(*) FROM activity WHERE created_day=?1 AND {where_sql}");
    Ok(conn.query_row(&sql, params![day], |r| r.get(0))?)
}

pub fn build(conn: &Connection, day: &str) -> AppResult<Digest> {
    NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|_| AppError::invalid("날짜 형식이 올바르지 않습니다(YYYY-MM-DD)"))?;
    let mut d = Digest { day: day.to_string(), ..Default::default() };
    let mut stmt = conn.prepare(
        "SELECT module_id, result, COUNT(*) FROM activity WHERE created_day=?1 AND module_id<>'core' GROUP BY module_id, result",
    )?;
    let rows = stmt.query_map(params![day], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?;
    for row in rows {
        let (m, res, n) = row?;
        let entry = match d.modules.iter_mut().position(|x| x.module_id == m) {
            Some(i) => &mut d.modules[i],
            None => {
                d.modules.push(ModuleCount { module_id: m.clone(), label: label(&m).to_string(), ..Default::default() });
                d.modules.last_mut().expect("방금 추가함")
            }
        };
        match res.as_str() {
            "completed" => entry.completed += n,
            "failed" => entry.failed += n,
            "waiting" => entry.waiting += n,
            _ => entry.skipped += n,
        }
    }
    let order = ["backup", "downloads", "autopaste", "workspace"];
    d.modules.sort_by_key(|m| order.iter().position(|o| *o == m.module_id).unwrap_or(99));
    d.completed = d.modules.iter().map(|m| m.completed).sum();
    d.failed = d.modules.iter().map(|m| m.failed).sum();
    d.pending_approvals = conn.query_row("SELECT COUNT(*) FROM jobs WHERE status='WAITING_APPROVAL'", [], |r| r.get(0))?;

    // 요약 문장(실제 건수만)
    let snaps = count(conn, day, "module_id='backup' AND action='backup.snapshot' AND result='completed'")?;
    let files: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT target) FROM activity WHERE created_day=?1 AND module_id='backup' AND action='backup.snapshot' AND result='completed'",
        params![day],
        |r| r.get(0),
    )?;
    let restores = count(conn, day, "module_id='backup' AND action='backup.restore' AND result='completed'")?;
    let moved = count(conn, day, "module_id='downloads' AND action='download.move' AND result='completed'")?;
    let undone = count(conn, day, "module_id='downloads' AND action='download.undo' AND result='completed'")?;
    let proposed = count(conn, day, "module_id='downloads' AND action='download.preview'")?;
    let cleans = count(conn, day, "module_id='autopaste' AND action='clipboard.clean' AND result='completed'")?;
    let polished = count(conn, day, "module_id='autopaste' AND action='clipboard.ai_polish' AND result='completed'")?;
    let starts = count(conn, day, "module_id='workspace' AND action='workspace.start'")?;
    let ends = count(conn, day, "module_id='workspace' AND action='workspace.end'")?;
    let mut h = Vec::new();
    if snaps > 0 {
        h.push(format!("파일 {files}개의 버전 {snaps}개를 백업했습니다"));
    }
    if restores > 0 {
        h.push(format!("백업에서 {restores}번 복원했습니다"));
    }
    if moved > 0 {
        h.push(format!("다운로드 파일 {moved}개를 정리했습니다{}", if undone > 0 { format!("(되돌림 {undone})") } else { String::new() }));
    }
    if proposed > 0 {
        h.push(format!("다운로드 이동 제안 {proposed}건을 만들었습니다"));
    }
    if cleans > 0 {
        h.push(format!("클립보드 텍스트를 {cleans}번 정리했습니다"));
    }
    if polished > 0 {
        h.push(format!("Claude Code로 {polished}번 다듬었습니다"));
    }
    if starts + ends > 0 {
        h.push(format!("업무 모드: 시작 {starts}번 · 퇴근 {ends}번"));
    }
    if d.failed > 0 {
        h.push(format!("실패 {}건 — 아래 목록을 확인하세요", d.failed));
    }
    if d.pending_approvals > 0 {
        h.push(format!("승인 센터에 확인 대기 {}건이 남아 있습니다", d.pending_approvals));
    }
    d.highlights = h;

    let mut fs = conn.prepare(
        "SELECT id, module_id, action, result, target, message, error_code, undo_ref, created_at FROM activity
         WHERE created_day=?1 AND result='failed' ORDER BY created_at DESC LIMIT 20",
    )?;
    let failures = fs.query_map(params![day], |r| {
        Ok(db::ActivityRow {
            id: r.get(0)?,
            module_id: r.get(1)?,
            action: r.get(2)?,
            result: r.get(3)?,
            target: r.get(4)?,
            message: r.get(5)?,
            error_code: r.get(6)?,
            undo_ref: r.get(7)?,
            created_at: r.get(8)?,
        })
    })?;
    d.failures = failures.collect::<Result<_, _>>()?;
    Ok(d)
}

/// 알림 한 줄 요약
pub fn one_line(d: &Digest) -> String {
    if d.highlights.is_empty() {
        return "오늘은 AutoDesk가 처리한 일이 없습니다".into();
    }
    d.highlights.iter().take(3).cloned().collect::<Vec<_>>().join(" · ")
}

pub fn markdown(d: &Digest) -> String {
    let mut s = format!("# AutoDesk 일일 요약 — {}\n\n", d.day);
    s += &format!("- 처리 완료 **{}**건 · 실패 **{}**건 · 확인 대기 **{}**건\n\n", d.completed, d.failed, d.pending_approvals);
    if !d.highlights.is_empty() {
        s += "## 한 일\n\n";
        for h in &d.highlights {
            s += &format!("- {h}\n");
        }
        s += "\n";
    }
    if !d.modules.is_empty() {
        s += "## 기능별\n\n| 기능 | 완료 | 실패 | 승인 대기 | 건너뜀 |\n| --- | --- | --- | --- | --- |\n";
        for m in &d.modules {
            s += &format!("| {} | {} | {} | {} | {} |\n", m.label, m.completed, m.failed, m.waiting, m.skipped);
        }
        s += "\n";
    }
    if !d.failures.is_empty() {
        s += "## 실패 목록\n\n";
        for f in &d.failures {
            let t = f.created_at.get(11..16).unwrap_or("");
            s += &format!("- {t} [{}] {} {}\n", label(&f.module_id), f.message, f.target.as_deref().unwrap_or(""));
        }
        s += "\n";
    }
    s += "_AutoDesk가 직접 처리한 기록만 집계했습니다._\n";
    s
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn html(d: &Digest) -> String {
    let mut body = format!(
        "<h1>AutoDesk 일일 요약 — {}</h1><p class=\"sum\"><b>{}</b> 처리 · <b>{}</b> 실패 · <b>{}</b> 확인 대기</p>",
        esc(&d.day), d.completed, d.failed, d.pending_approvals
    );
    if !d.highlights.is_empty() {
        body += "<h2>한 일</h2><ul>";
        for h in &d.highlights {
            body += &format!("<li>{}</li>", esc(h));
        }
        body += "</ul>";
    }
    if !d.modules.is_empty() {
        body += "<h2>기능별</h2><table><tr><th>기능</th><th>완료</th><th>실패</th><th>승인 대기</th><th>건너뜀</th></tr>";
        for m in &d.modules {
            body += &format!("<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>", esc(&m.label), m.completed, m.failed, m.waiting, m.skipped);
        }
        body += "</table>";
    }
    if !d.failures.is_empty() {
        body += "<h2>실패 목록</h2><ul>";
        for f in &d.failures {
            body += &format!("<li>{} [{}] {} <small>{}</small></li>", esc(f.created_at.get(11..16).unwrap_or("")), esc(label(&f.module_id)), esc(&f.message), esc(f.target.as_deref().unwrap_or("")));
        }
        body += "</ul>";
    }
    format!(
        "<!doctype html><html lang=\"ko\"><head><meta charset=\"utf-8\"><title>AutoDesk 일일 요약 {}</title><style>body{{font-family:'Malgun Gothic',system-ui,sans-serif;max-width:760px;margin:32px auto;color:#1f2328}}h1{{font-size:22px}}h2{{font-size:16px;margin-top:24px}}table{{border-collapse:collapse;width:100%}}td,th{{border-bottom:1px solid #eee;padding:6px 8px;text-align:left}}.sum{{color:#646b75}}</style></head><body>{}<p style=\"color:#999\">AutoDesk가 직접 처리한 기록만 집계했습니다.</p></body></html>",
        esc(&d.day), body
    )
}

pub fn settings(core: &Core) -> DigestSettings {
    db::get_setting_or(&core.db(), "digest", DigestSettings::default())
}

/// 엔진 스레드에서 1분마다 호출: 알림 시각이 지났고 오늘 아직 안 보냈으면 한 번 알린다.
pub fn tick(core: &Arc<Core>) {
    let s = settings(core);
    if !s.notify_enabled {
        return;
    }
    let Ok(at) = NaiveTime::parse_from_str(&s.notify_time, "%H:%M") else { return };
    let now = Local::now();
    if now.time() < at {
        return;
    }
    let today = now.format("%Y-%m-%d").to_string();
    let last: String = db::get_setting_or(&core.db(), "digest_last_sent", String::new());
    if last == today {
        return;
    }
    // 오늘 이미 처리했다고 기록(실패해도 같은 날 반복 알림 방지)
    let _ = db::set_setting(&core.db(), "digest_last_sent", &today);
    let Ok(d) = build(&core.db(), &today) else { return };
    if s.skip_if_empty && d.completed == 0 && d.failed == 0 {
        return;
    }
    core.notify("AutoDesk 오늘의 요약", &one_line(&d));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(conn: &Connection) {
        let add = |m: &str, a: &str, r: &str, t: Option<&str>| db::add_activity(conn, m, a, r, t, "msg", None, None).unwrap();
        add("backup", "backup.snapshot", "completed", Some("a.md"));
        add("backup", "backup.snapshot", "completed", Some("a.md"));
        add("backup", "backup.snapshot", "completed", Some("b.txt"));
        add("backup", "backup.snapshot", "failed", Some("c.txt"));
        add("downloads", "download.move", "completed", Some("x.pdf"));
        add("downloads", "download.preview", "waiting", Some("y.exe"));
        add("autopaste", "clipboard.clean", "completed", None);
        add("workspace", "workspace.end", "completed", None);
        add("core", "master", "completed", None); // 시스템 기록은 집계에서 제외
    }

    #[test]
    fn counts_only_real_activity() {
        let conn = db::open_in_memory().unwrap();
        seed(&conn);
        let d = build(&conn, &db::today_local()).unwrap();
        assert_eq!(d.completed, 6);
        assert_eq!(d.failed, 1);
        assert_eq!(d.modules[0].module_id, "backup");
        assert_eq!(d.modules[0].completed, 3);
        assert!(d.highlights.iter().any(|h| h.contains("파일 2개의 버전 3개")));
        assert!(d.highlights.iter().any(|h| h.contains("다운로드 파일 1개")));
        assert!(d.highlights.iter().any(|h| h.contains("퇴근 1번")));
        assert_eq!(d.failures.len(), 1);
    }

    #[test]
    fn empty_day_and_formats() {
        let conn = db::open_in_memory().unwrap();
        let d = build(&conn, "2026-01-01").unwrap();
        assert_eq!(d.completed, 0);
        assert!(one_line(&d).contains("없습니다"));
        seed(&conn);
        let d = build(&conn, &db::today_local()).unwrap();
        let md = markdown(&d);
        assert!(md.contains("| AutoBackup | 3 | 1 |"));
        let h = html(&d);
        assert!(h.contains("<td>AutoBackup</td>"));
        assert!(build(&conn, "2026/01/01").is_err());
    }

    #[test]
    fn settings_validation() {
        assert!(validate(DigestSettings { notify_time: "25:00".into(), ..Default::default() }).is_err());
        assert!(validate(DigestSettings { notify_time: "07:30".into(), ..Default::default() }).is_ok());
    }
}
