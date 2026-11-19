//! AutoBackup Guardian: 감시 폴더의 파일이 실제로 바뀌면(SHA-256 비교) 앱 데이터 폴더에 스냅샷을 남긴다.
//! 원본은 절대 수정하지 않는다. 복원은 사용자가 명시적으로 실행할 때만 한다.

use crate::db;
use crate::error::{AppError, AppResult};
use crate::fsutil;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BackupConfig {
    pub max_versions: usize,
    pub max_file_size_mb: u64,
    pub max_total_mb: u64,
    /// 비어 있으면 모든 확장자
    pub extensions: Vec<String>,
    pub excludes: Vec<String>,
}

pub const DEFAULT_EXCLUDES: &[&str] = &[
    "~$*", "*.tmp", "*.temp", "*.swp", "*.lock", "*.crdownload", "*.part", ".DS_Store", "Thumbs.db", "desktop.ini",
    "node_modules/**", ".git/**", ".venv/**", ".cache/**", "target/**", "AutoDeskData/**",
];

pub const DEFAULT_EXTENSIONS: &[&str] = &[
    "txt", "md", "csv", "docx", "xlsx", "pptx", "hwp", "hwpx", "js", "ts", "jsx", "tsx", "html", "css", "json", "xml", "yaml",
    "yml", "java", "py", "rs", "sql", "kt", "go", "c", "cpp", "h",
];

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            max_versions: 10,
            max_file_size_mb: 50,
            max_total_mb: 2048,
            extensions: DEFAULT_EXTENSIONS.iter().map(|s| s.to_string()).collect(),
            excludes: DEFAULT_EXCLUDES.iter().map(|s| s.to_string()).collect(),
        }
    }
}

pub fn is_candidate(path: &Path, cfg: &BackupConfig) -> bool {
    if cfg.excludes.iter().any(|p| fsutil::matches_pattern(path, p)) {
        return false;
    }
    if cfg.extensions.is_empty() {
        return true;
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    cfg.extensions.iter().any(|e| e.trim_start_matches('.').eq_ignore_ascii_case(&ext))
}

pub enum BackupOutcome {
    Created { size: u64 },
    Unchanged,
    TooLarge(u64),
}

fn latest_hash(conn: &Connection, original: &str) -> AppResult<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT sha256 FROM file_versions WHERE original_path=?1 ORDER BY created_at DESC LIMIT 1",
            [original],
            |r| r.get(0),
        )
        .optional()?)
}

#[derive(Serialize, Deserialize, Default)]
struct Manifest {
    original_path: String,
    relative_path: String,
    versions: Vec<ManifestEntry>,
}
#[derive(Serialize, Deserialize)]
struct ManifestEntry {
    file: String,
    sha256: String,
    size: u64,
    created_at: String,
}

/// 파일 하나를 백업한다. DB가 손상돼도 식별할 수 있도록 폴더마다 manifest.json을 함께 쓴다.
pub fn backup_file(
    conn: &Connection,
    backups_root: &Path,
    target_id: &str,
    target_root: &Path,
    file: &Path,
    cfg: &BackupConfig,
) -> AppResult<BackupOutcome> {
    let meta = fs::metadata(file)?;
    if meta.len() > cfg.max_file_size_mb * 1024 * 1024 {
        return Ok(BackupOutcome::TooLarge(meta.len()));
    }
    let original = file.to_string_lossy().to_string();
    let hash = fsutil::sha256_file(file)?;
    if latest_hash(conn, &original)?.as_deref() == Some(hash.as_str()) {
        return Ok(BackupOutcome::Unchanged);
    }
    let relative = file.strip_prefix(target_root).unwrap_or(file).to_string_lossy().replace('\\', "/");
    let dir = backups_root.join(target_id).join(&fsutil::sha256_str(&relative)[..16]);
    let ts = Utc::now();
    let snap_name = format!("{}_{}.snap", ts.format("%Y%m%dT%H%M%S%3f"), &hash[..8]);
    let snap = dir.join(&snap_name);
    fsutil::atomic_copy(file, &snap)?;

    let manifest_path = dir.join("manifest.json");
    let mut manifest: Manifest = fs::read_to_string(&manifest_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    manifest.original_path = original.clone();
    manifest.relative_path = relative.clone();
    manifest.versions.push(ManifestEntry { file: snap_name, sha256: hash.clone(), size: meta.len(), created_at: ts.to_rfc3339() });
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;

    let modified = meta.modified().ok().map(|m| DateTime::<Utc>::from(m).to_rfc3339());
    let id = db::new_id();
    conn.execute(
        "INSERT INTO file_versions(id, watch_target_id, original_path, relative_path, snapshot_path, sha256, file_size, modified_at, created_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![id, target_id, original, relative, snap.to_string_lossy(), hash, meta.len() as i64, modified, ts.to_rfc3339()],
    )?;
    enforce_retention(conn, &original, cfg)?;
    Ok(BackupOutcome::Created { size: meta.len() })
}

fn delete_version(conn: &Connection, id: &str, snapshot: &str) -> AppResult<()> {
    let _ = fs::remove_file(snapshot);
    conn.execute("DELETE FROM file_versions WHERE id=?1", [id])?;
    Ok(())
}

/// 파일별 보관 개수와 전체 용량 제한을 적용한다. 각 파일의 최신 버전은 남긴다.
pub fn enforce_retention(conn: &Connection, original: &str, cfg: &BackupConfig) -> AppResult<usize> {
    let mut removed = 0;
    let keep = cfg.max_versions.max(1) as i64;
    let old: Vec<(String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, snapshot_path FROM file_versions WHERE original_path=?1 ORDER BY created_at DESC LIMIT -1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![original, keep], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    for (id, snap) in old {
        delete_version(conn, &id, &snap)?;
        removed += 1;
    }
    let limit = (cfg.max_total_mb * 1024 * 1024) as i64;
    loop {
        let total: i64 = conn.query_row("SELECT COALESCE(SUM(file_size),0) FROM file_versions", [], |r| r.get(0))?;
        if total <= limit {
            break;
        }
        // 버전이 2개 이상인 파일 중 가장 오래된 스냅샷부터 지운다.
        let victim: Option<(String, String)> = conn
            .query_row(
                "SELECT id, snapshot_path FROM file_versions v
                 WHERE (SELECT COUNT(*) FROM file_versions w WHERE w.original_path=v.original_path) > 1
                 ORDER BY created_at ASC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match victim {
            Some((id, snap)) => {
                delete_version(conn, &id, &snap)?;
                removed += 1;
            }
            None => break,
        }
    }
    Ok(removed)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionRow {
    pub id: String,
    pub original_path: String,
    pub relative_path: String,
    pub sha256: String,
    pub file_size: i64,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackedFile {
    pub original_path: String,
    pub relative_path: String,
    pub versions: i64,
    pub last_backup_at: String,
    pub exists: bool,
}

pub fn list_backed_files(conn: &Connection, query: Option<&str>) -> AppResult<Vec<BackedFile>> {
    let like = format!("%{}%", query.unwrap_or(""));
    let mut stmt = conn.prepare(
        "SELECT original_path, MAX(relative_path), COUNT(*), MAX(created_at) FROM file_versions
         WHERE original_path LIKE ?1 GROUP BY original_path ORDER BY MAX(created_at) DESC LIMIT 300",
    )?;
    let rows = stmt.query_map([like], |r| {
        let p: String = r.get(0)?;
        Ok(BackedFile {
            exists: Path::new(&p).exists(),
            original_path: p,
            relative_path: r.get(1)?,
            versions: r.get(2)?,
            last_backup_at: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn list_versions(conn: &Connection, original: &str) -> AppResult<Vec<VersionRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, original_path, relative_path, sha256, file_size, created_at FROM file_versions WHERE original_path=?1 ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([original], |r| {
        Ok(VersionRow {
            id: r.get(0)?,
            original_path: r.get(1)?,
            relative_path: r.get(2)?,
            sha256: r.get(3)?,
            file_size: r.get(4)?,
            created_at: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub struct VersionInfo {
    pub watch_target_id: String,
    pub original_path: PathBuf,
    pub snapshot_path: PathBuf,
    pub sha256: String,
    pub size: i64,
}

pub fn get_version(conn: &Connection, id: &str) -> AppResult<VersionInfo> {
    conn.query_row(
        "SELECT watch_target_id, original_path, snapshot_path, sha256, file_size FROM file_versions WHERE id=?1",
        [id],
        |r| {
            Ok(VersionInfo {
                watch_target_id: r.get(0)?,
                original_path: PathBuf::from(r.get::<_, String>(1)?),
                snapshot_path: PathBuf::from(r.get::<_, String>(2)?),
                sha256: r.get(3)?,
                size: r.get(4)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("백업 버전을 찾을 수 없습니다"))
}

const TEXT_EXT: &[&str] = &[
    "txt", "md", "csv", "js", "ts", "jsx", "tsx", "html", "css", "json", "xml", "yaml", "yml", "java", "py", "rs", "sql", "kt",
    "go", "c", "cpp", "h", "log", "ini", "toml",
];

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all_fields = "camelCase")]
pub enum DiffResult {
    Text { diff: String, identical: bool },
    Binary { snapshot_size: i64, snapshot_hash: String, current_size: Option<u64>, current_hash: Option<String> },
}

/// 스냅샷과 현재 파일을 비교한다. 텍스트(2MB 이하)는 unified diff, 그 외는 메타데이터 비교.
pub fn diff_with_current(v: &VersionInfo) -> AppResult<DiffResult> {
    let ext = v.original_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let current_meta = fs::metadata(&v.original_path).ok();
    let small = v.size < 2 * 1024 * 1024 && current_meta.as_ref().map(|m| m.len() < 2 * 1024 * 1024).unwrap_or(true);
    if TEXT_EXT.contains(&ext.as_str()) && small {
        let old = fs::read(&v.snapshot_path)?;
        let new = fs::read(&v.original_path).unwrap_or_default();
        if let (Ok(old), Ok(new)) = (String::from_utf8(old), String::from_utf8(new)) {
            let diff = similar::TextDiff::from_lines(&old, &new)
                .unified_diff()
                .context_radius(3)
                .header("백업 버전", "현재 파일")
                .to_string();
            return Ok(DiffResult::Text { identical: old == new, diff });
        }
    }
    Ok(DiffResult::Binary {
        snapshot_size: v.size,
        snapshot_hash: v.sha256.clone(),
        current_size: current_meta.map(|m| m.len()),
        current_hash: fsutil::sha256_file(&v.original_path).ok(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Connection, tempfile::TempDir) {
        (db::open_in_memory().unwrap(), tempfile::tempdir().unwrap())
    }

    #[test]
    fn candidate_filtering() {
        let c = BackupConfig::default();
        assert!(is_candidate(Path::new("w/notes.md"), &c));
        assert!(!is_candidate(Path::new("w/~$doc.docx"), &c));
        assert!(!is_candidate(Path::new("w/node_modules/a.js"), &c));
        assert!(!is_candidate(Path::new("w/movie.mp4"), &c));
    }

    #[test]
    fn creates_snapshot_only_on_real_change() {
        let (conn, d) = setup();
        let root = d.path().join("work");
        let backups = d.path().join("backups");
        fs::create_dir_all(&root).unwrap();
        let f = root.join("a.txt");
        fs::write(&f, "v1").unwrap();
        let cfg = BackupConfig::default();
        assert!(matches!(backup_file(&conn, &backups, "t1", &root, &f, &cfg).unwrap(), BackupOutcome::Created { .. }));
        // 같은 내용으로 다시 저장해도 스냅샷이 늘지 않는다(중복 방지).
        assert!(matches!(backup_file(&conn, &backups, "t1", &root, &f, &cfg).unwrap(), BackupOutcome::Unchanged));
        fs::write(&f, "v2").unwrap();
        assert!(matches!(backup_file(&conn, &backups, "t1", &root, &f, &cfg).unwrap(), BackupOutcome::Created { .. }));
        let versions = list_versions(&conn, &f.to_string_lossy()).unwrap();
        assert_eq!(versions.len(), 2);
        // 원본은 그대로다.
        assert_eq!(fs::read_to_string(&f).unwrap(), "v2");
        // manifest가 있다.
        let v = get_version(&conn, &versions[0].id).unwrap();
        assert!(v.snapshot_path.parent().unwrap().join("manifest.json").exists());
    }

    #[test]
    fn retention_keeps_max_versions() {
        let (conn, d) = setup();
        let root = d.path().join("w");
        fs::create_dir_all(&root).unwrap();
        let f = root.join("a.txt");
        let cfg = BackupConfig { max_versions: 3, ..Default::default() };
        for i in 0..6 {
            fs::write(&f, format!("v{i}")).unwrap();
            backup_file(&conn, &d.path().join("b"), "t", &root, &f, &cfg).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let vs = list_versions(&conn, &f.to_string_lossy()).unwrap();
        assert_eq!(vs.len(), 3);
        let snaps = fs::read_dir(get_version(&conn, &vs[0].id).unwrap().snapshot_path.parent().unwrap())
            .unwrap()
            .filter(|e| e.as_ref().unwrap().path().extension().map(|x| x == "snap").unwrap_or(false))
            .count();
        assert_eq!(snaps, 3);
    }

    #[test]
    fn too_large_is_skipped() {
        let (conn, d) = setup();
        let f = d.path().join("big.txt");
        fs::write(&f, vec![b'a'; 2 * 1024 * 1024]).unwrap();
        let cfg = BackupConfig { max_file_size_mb: 1, ..Default::default() };
        assert!(matches!(backup_file(&conn, &d.path().join("b"), "t", d.path(), &f, &cfg).unwrap(), BackupOutcome::TooLarge(_)));
    }

    #[test]
    fn text_diff() {
        let (conn, d) = setup();
        let f = d.path().join("a.md");
        fs::write(&f, "line1\nline2\n").unwrap();
        backup_file(&conn, &d.path().join("b"), "t", d.path(), &f, &BackupConfig::default()).unwrap();
        fs::write(&f, "line1\nchanged\n").unwrap();
        let v = list_versions(&conn, &f.to_string_lossy()).unwrap();
        match diff_with_current(&get_version(&conn, &v[0].id).unwrap()).unwrap() {
            DiffResult::Text { diff, identical } => {
                assert!(!identical);
                assert!(diff.contains("-line2"));
                assert!(diff.contains("+changed"));
            }
            _ => panic!("expected text diff"),
        }
    }

    #[test]
    fn diff_result_matches_frontend_contract() {
        // 프런트(api.ts)의 DiffResult 타입과 같은 모양이어야 한다.
        let v = serde_json::to_value(DiffResult::Binary {
            snapshot_size: 1,
            snapshot_hash: "a".into(),
            current_size: None,
            current_hash: None,
        })
        .unwrap();
        assert_eq!(v["kind"], "Binary");
        assert_eq!(v["snapshotSize"], 1);
        let t = serde_json::to_value(DiffResult::Text { diff: "d".into(), identical: true }).unwrap();
        assert_eq!(t["kind"], "Text");
        assert_eq!(t["identical"], true);
    }
}
