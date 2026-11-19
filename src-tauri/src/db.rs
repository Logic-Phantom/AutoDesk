//! SQLite 저장소. 마이그레이션 기반 스키마와 짧은 트랜잭션 단위의 조회/기록 함수를 제공한다.
//! 클립보드 원문, API 키, 비밀번호는 어떤 테이블에도 저장하지 않는다.

use crate::error::{AppError, AppResult};
use chrono::{Local, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;

const MIGRATIONS: &[(i64, &str)] = &[(
    1,
    r#"
CREATE TABLE app_settings (
  key TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE automation_modules (
  id TEXT PRIMARY KEY,
  enabled INTEGER NOT NULL,
  status TEXT NOT NULL,
  config_json TEXT NOT NULL,
  last_started_at TEXT,
  last_run_at TEXT,
  last_error_code TEXT,
  consecutive_errors INTEGER NOT NULL DEFAULT 0,
  updated_at TEXT NOT NULL
);
CREATE TABLE watch_targets (
  id TEXT PRIMARY KEY,
  module_id TEXT NOT NULL,
  path TEXT NOT NULL,
  recursive INTEGER NOT NULL,
  enabled INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(module_id, path)
);
CREATE TABLE jobs (
  id TEXT PRIMARY KEY,
  module_id TEXT NOT NULL,
  job_type TEXT NOT NULL,
  status TEXT NOT NULL,
  idempotency_key TEXT NOT NULL UNIQUE,
  payload_json TEXT NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  error_code TEXT,
  error_message TEXT,
  created_at TEXT NOT NULL,
  started_at TEXT,
  completed_at TEXT
);
CREATE INDEX idx_jobs_status ON jobs(status);
CREATE TABLE activity (
  id TEXT PRIMARY KEY,
  module_id TEXT NOT NULL,
  action TEXT NOT NULL,
  result TEXT NOT NULL,
  target TEXT,
  message TEXT NOT NULL,
  error_code TEXT,
  undo_ref TEXT,
  created_at TEXT NOT NULL,
  created_day TEXT NOT NULL
);
CREATE INDEX idx_activity_day ON activity(created_day);
CREATE TABLE file_versions (
  id TEXT PRIMARY KEY,
  watch_target_id TEXT NOT NULL,
  original_path TEXT NOT NULL,
  relative_path TEXT NOT NULL,
  snapshot_path TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  file_size INTEGER NOT NULL,
  modified_at TEXT,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_versions_path ON file_versions(original_path);
CREATE TABLE file_moves (
  id TEXT PRIMARY KEY,
  module_id TEXT NOT NULL,
  original_path TEXT NOT NULL,
  new_path TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  undo_available INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  undone_at TEXT
);
"#,
), (
    2,
    r#"
CREATE TABLE log_offsets (
  target_id TEXT PRIMARY KEY,
  offset INTEGER NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE log_issues (
  fingerprint TEXT NOT NULL,
  target_id TEXT NOT NULL,
  level TEXT NOT NULL,
  sample TEXT NOT NULL,
  context TEXT NOT NULL,
  count INTEGER NOT NULL,
  first_seen TEXT NOT NULL,
  last_seen TEXT NOT NULL,
  last_notified TEXT,
  PRIMARY KEY (fingerprint, target_id)
);
CREATE INDEX idx_log_issues_last ON log_issues(last_seen);
"#,
)];

pub fn now() -> String {
    Utc::now().to_rfc3339()
}

pub fn today_local() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn open(path: &Path) -> AppResult<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")?;
    migrate(&conn, path)?;
    Ok(conn)
}

#[cfg(test)]
pub fn open_in_memory() -> AppResult<Connection> {
    let conn = Connection::open_in_memory()?;
    migrate(&conn, Path::new(":memory:"))?;
    Ok(conn)
}

fn migrate(conn: &Connection, path: &Path) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);",
    )?;
    let current: i64 = conn.query_row("SELECT COALESCE(MAX(version),0) FROM schema_migrations", [], |r| r.get(0))?;
    let pending: Vec<_> = MIGRATIONS.iter().filter(|(v, _)| *v > current).collect();
    if pending.is_empty() {
        return Ok(());
    }
    // 기존 DB가 있으면 마이그레이션 전에 백업한다(실패 시 수동 롤백용).
    if current > 0 && path.exists() {
        let backup = path.with_extension(format!("v{current}.bak"));
        let _ = std::fs::copy(path, backup);
    }
    for (version, sql) in pending {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)
            .map_err(|e| AppError::new("MIGRATION_FAILED", format!("DB 마이그레이션 {version} 실패: {e}")))?;
        tx.execute("INSERT INTO schema_migrations(version, applied_at) VALUES(?1, ?2)", params![version, now()])?;
        tx.commit()?;
    }
    Ok(())
}

// ---------- settings ----------

pub fn get_setting<T: DeserializeOwned>(conn: &Connection, key: &str) -> AppResult<Option<T>> {
    let raw: Option<String> = conn
        .query_row("SELECT value_json FROM app_settings WHERE key=?1", [key], |r| r.get(0))
        .optional()?;
    match raw {
        Some(s) => Ok(Some(serde_json::from_str(&s)?)),
        None => Ok(None),
    }
}

pub fn get_setting_or<T: DeserializeOwned>(conn: &Connection, key: &str, default: T) -> T {
    get_setting(conn, key).ok().flatten().unwrap_or(default)
}

pub fn set_setting<T: Serialize>(conn: &Connection, key: &str, value: &T) -> AppResult<()> {
    conn.execute(
        "INSERT INTO app_settings(key, value_json, updated_at) VALUES(?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json, updated_at=excluded.updated_at",
        params![key, serde_json::to_string(value)?, now()],
    )?;
    Ok(())
}

// ---------- modules ----------

pub struct ModuleRow {
    pub enabled: bool,
    pub config_json: String,
    pub last_run_at: Option<String>,
    pub last_error_code: Option<String>,
    pub consecutive_errors: i64,
    pub status: String,
}

pub fn ensure_module(conn: &Connection, id: &str, default_enabled: bool, default_config: &str) -> AppResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO automation_modules(id, enabled, status, config_json, updated_at) VALUES(?1, ?2, 'DISABLED', ?3, ?4)",
        params![id, default_enabled as i64, default_config, now()],
    )?;
    Ok(())
}

pub fn get_module(conn: &Connection, id: &str) -> AppResult<ModuleRow> {
    conn.query_row(
        "SELECT enabled, config_json, last_run_at, last_error_code, consecutive_errors, status FROM automation_modules WHERE id=?1",
        [id],
        |r| {
            Ok(ModuleRow {
                enabled: r.get::<_, i64>(0)? != 0,
                config_json: r.get(1)?,
                last_run_at: r.get(2)?,
                last_error_code: r.get(3)?,
                consecutive_errors: r.get(4)?,
                status: r.get(5)?,
            })
        },
    )
    .map_err(Into::into)
}

pub fn set_module_enabled(conn: &Connection, id: &str, enabled: bool) -> AppResult<()> {
    conn.execute(
        "UPDATE automation_modules SET enabled=?2, consecutive_errors=0, last_error_code=NULL, updated_at=?3 WHERE id=?1",
        params![id, enabled as i64, now()],
    )?;
    Ok(())
}

pub fn set_module_status(conn: &Connection, id: &str, status: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE automation_modules SET status=?2, updated_at=?3,
           last_started_at = CASE WHEN ?2='RUNNING' AND status<>'RUNNING' THEN ?3 ELSE last_started_at END
         WHERE id=?1",
        params![id, status, now()],
    )?;
    Ok(())
}

pub fn set_module_config(conn: &Connection, id: &str, config_json: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE automation_modules SET config_json=?2, updated_at=?3 WHERE id=?1",
        params![id, config_json, now()],
    )?;
    Ok(())
}

/// 모듈 실행 결과를 반영한다. 연속 오류 횟수를 반환한다.
pub fn record_module_run(conn: &Connection, id: &str, error_code: Option<&str>) -> AppResult<i64> {
    match error_code {
        None => conn.execute(
            "UPDATE automation_modules SET last_run_at=?2, consecutive_errors=0, last_error_code=NULL WHERE id=?1",
            params![id, now()],
        )?,
        Some(code) => conn.execute(
            "UPDATE automation_modules SET last_run_at=?2, consecutive_errors=consecutive_errors+1, last_error_code=?3 WHERE id=?1",
            params![id, now(), code],
        )?,
    };
    Ok(conn.query_row("SELECT consecutive_errors FROM automation_modules WHERE id=?1", [id], |r| r.get(0))?)
}

// ---------- watch targets ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchTarget {
    pub id: String,
    pub module_id: String,
    pub path: String,
    pub recursive: bool,
    pub enabled: bool,
}

pub fn list_watch_targets(conn: &Connection, module_id: Option<&str>) -> AppResult<Vec<WatchTarget>> {
    let mut stmt = conn.prepare(
        "SELECT id, module_id, path, recursive, enabled FROM watch_targets WHERE (?1 IS NULL OR module_id=?1) ORDER BY created_at",
    )?;
    let rows = stmt.query_map([module_id], |r| {
        Ok(WatchTarget {
            id: r.get(0)?,
            module_id: r.get(1)?,
            path: r.get(2)?,
            recursive: r.get::<_, i64>(3)? != 0,
            enabled: r.get::<_, i64>(4)? != 0,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn add_watch_target(conn: &Connection, module_id: &str, path: &str, recursive: bool) -> AppResult<String> {
    let id = new_id();
    let n = conn.execute(
        "INSERT OR IGNORE INTO watch_targets(id, module_id, path, recursive, enabled, created_at) VALUES(?1, ?2, ?3, ?4, 1, ?5)",
        params![id, module_id, path, recursive as i64, now()],
    )?;
    if n == 0 {
        return Err(AppError::new("ALREADY_EXISTS", "이미 등록된 폴더입니다"));
    }
    Ok(id)
}

pub fn remove_watch_target(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM watch_targets WHERE id=?1", [id])?;
    Ok(())
}

// ---------- jobs ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRow {
    pub id: String,
    pub module_id: String,
    pub job_type: String,
    pub status: String,
    pub payload_json: String,
    pub attempts: i64,
    pub created_at: String,
}

/// 같은 idempotency key의 작업이 이미 있으면 None을 반환한다(중복 작업 병합).
pub fn insert_job(conn: &Connection, module_id: &str, job_type: &str, status: &str, key: &str, payload: &str) -> AppResult<Option<String>> {
    let id = new_id();
    let n = conn.execute(
        "INSERT OR IGNORE INTO jobs(id, module_id, job_type, status, idempotency_key, payload_json, created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![id, module_id, job_type, status, key, payload, now()],
    )?;
    Ok(if n == 1 { Some(id) } else { None })
}

pub fn update_job_status(conn: &Connection, id: &str, status: &str, error: Option<&AppError>) -> AppResult<()> {
    let ts = now();
    conn.execute(
        "UPDATE jobs SET status=?2,
           attempts = attempts + CASE WHEN ?2='RUNNING' THEN 1 ELSE 0 END,
           started_at = CASE WHEN ?2='RUNNING' THEN ?3 ELSE started_at END,
           completed_at = CASE WHEN ?2 IN ('COMPLETED','FAILED','SKIPPED','CANCELLED') THEN ?3 ELSE completed_at END,
           error_code=?4, error_message=?5
         WHERE id=?1",
        params![id, status, ts, error.map(|e| e.code), error.map(|e| e.message.clone())],
    )?;
    Ok(())
}

pub fn jobs_with_status(conn: &Connection, statuses: &[&str], module_id: Option<&str>) -> AppResult<Vec<JobRow>> {
    let placeholders = statuses.iter().map(|s| format!("'{}'", s.replace('\'', ""))).collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT id, module_id, job_type, status, payload_json, attempts, created_at FROM jobs
         WHERE status IN ({placeholders}) AND (?1 IS NULL OR module_id=?1) ORDER BY created_at"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([module_id], |r| {
        Ok(JobRow {
            id: r.get(0)?,
            module_id: r.get(1)?,
            job_type: r.get(2)?,
            status: r.get(3)?,
            payload_json: r.get(4)?,
            attempts: r.get(5)?,
            created_at: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get_job(conn: &Connection, id: &str) -> AppResult<JobRow> {
    conn.query_row(
        "SELECT id, module_id, job_type, status, payload_json, attempts, created_at FROM jobs WHERE id=?1",
        [id],
        |r| {
            Ok(JobRow {
                id: r.get(0)?,
                module_id: r.get(1)?,
                job_type: r.get(2)?,
                status: r.get(3)?,
                payload_json: r.get(4)?,
                attempts: r.get(5)?,
                created_at: r.get(6)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("작업을 찾을 수 없습니다"))
}

/// 모듈/전역 일시정지된 작업 상태를 일괄 변경한다.
pub fn move_jobs(conn: &Connection, from: &str, to: &str, module_id: Option<&str>) -> AppResult<usize> {
    Ok(conn.execute(
        "UPDATE jobs SET status=?2 WHERE status=?1 AND (?3 IS NULL OR module_id=?3)",
        params![from, to, module_id],
    )?)
}

// ---------- activity ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityRow {
    pub id: String,
    pub module_id: String,
    pub action: String,
    pub result: String,
    pub target: Option<String>,
    pub message: String,
    pub error_code: Option<String>,
    pub undo_ref: Option<String>,
    pub created_at: String,
}

#[allow(clippy::too_many_arguments)]
pub fn add_activity(
    conn: &Connection,
    module_id: &str,
    action: &str,
    result: &str,
    target: Option<&str>,
    message: &str,
    error_code: Option<&str>,
    undo_ref: Option<&str>,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO activity(id, module_id, action, result, target, message, error_code, undo_ref, created_at, created_day)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![new_id(), module_id, action, result, target, message, error_code, undo_ref, now(), today_local()],
    )?;
    // 이력은 최근 5000건만 유지한다.
    conn.execute(
        "DELETE FROM activity WHERE id IN (SELECT id FROM activity ORDER BY created_at DESC LIMIT -1 OFFSET 5000)",
        [],
    )?;
    Ok(())
}

pub fn list_activity(conn: &Connection, result: Option<&str>, module_id: Option<&str>, limit: i64) -> AppResult<Vec<ActivityRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, module_id, action, result, target, message, error_code, undo_ref, created_at FROM activity
         WHERE (?1 IS NULL OR result=?1) AND (?2 IS NULL OR module_id=?2)
         ORDER BY created_at DESC LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![result, module_id, limit], |r| {
        Ok(ActivityRow {
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
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn count_activity_today(conn: &Connection, result: &str, module_id: Option<&str>) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM activity WHERE created_day=?1 AND result=?2 AND (?3 IS NULL OR module_id=?3)",
        params![today_local(), result, module_id],
        |r| r.get(0),
    )?)
}

/// 종료 시 WAL 체크포인트로 DB를 안전하게 정리한다.
pub fn checkpoint(conn: &Connection) {
    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_and_settings_roundtrip() {
        let c = open_in_memory().unwrap();
        set_setting(&c, "master_enabled", &false).unwrap();
        assert_eq!(get_setting::<bool>(&c, "master_enabled").unwrap(), Some(false));
        assert!(get_setting_or(&c, "missing", true));
    }

    #[test]
    fn job_idempotency_merges_duplicates() {
        let c = open_in_memory().unwrap();
        let a = insert_job(&c, "m", "t", "QUEUED", "k1", "{}").unwrap();
        let b = insert_job(&c, "m", "t", "QUEUED", "k1", "{}").unwrap();
        assert!(a.is_some());
        assert!(b.is_none());
    }

    #[test]
    fn job_pause_and_resume_transitions() {
        let c = open_in_memory().unwrap();
        let id = insert_job(&c, "m", "t", "QUEUED", "k", "{}").unwrap().unwrap();
        assert_eq!(move_jobs(&c, "QUEUED", "PAUSED_GLOBAL", None).unwrap(), 1);
        assert_eq!(get_job(&c, &id).unwrap().status, "PAUSED_GLOBAL");
        move_jobs(&c, "PAUSED_GLOBAL", "QUEUED", None).unwrap();
        update_job_status(&c, &id, "RUNNING", None).unwrap();
        let j = get_job(&c, &id).unwrap();
        assert_eq!(j.status, "RUNNING");
        assert_eq!(j.attempts, 1);
    }

    #[test]
    fn module_error_counter() {
        let c = open_in_memory().unwrap();
        ensure_module(&c, "x", true, "{}").unwrap();
        assert_eq!(record_module_run(&c, "x", Some("IO_ERROR")).unwrap(), 1);
        assert_eq!(record_module_run(&c, "x", Some("IO_ERROR")).unwrap(), 2);
        assert_eq!(record_module_run(&c, "x", None).unwrap(), 0);
    }
}
