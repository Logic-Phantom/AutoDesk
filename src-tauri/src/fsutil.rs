//! 파일 작업 공통 유틸: 해시, 안정성 확인, 충돌 없는 이름, 안전 이동, 경로 검증, 제외 패턴.

use crate::error::{AppError, AppResult};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn sha256_file(path: &Path) -> AppResult<String> {
    let mut f = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn sha256_str(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

/// 파일 크기·수정 시각이 `wait` 동안 변하지 않으면 안정된 것으로 본다.
pub fn is_stable(path: &Path, wait: Duration) -> bool {
    let a = match fs::metadata(path) {
        Ok(m) => (m.len(), m.modified().ok()),
        Err(_) => return false,
    };
    std::thread::sleep(wait);
    match fs::metadata(path) {
        Ok(m) => (m.len(), m.modified().ok()) == a,
        Err(_) => false,
    }
}

/// `name.ext`가 있으면 `name (2).ext`, `name (3).ext` … 처럼 충돌하지 않는 경로를 만든다.
pub fn unique_path(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let p = Path::new(file_name);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or(file_name);
    let ext = p.extension().and_then(|s| s.to_str());
    for i in 2..10_000 {
        let name = match ext {
            Some(e) => format!("{stem} ({i}).{e}"),
            None => format!("{stem} ({i})"),
        };
        let c = dir.join(name);
        if !c.exists() {
            return c;
        }
    }
    dir.join(format!("{}-{}", uuid::Uuid::new_v4(), file_name))
}

/// 덮어쓰기 없이 이동한다. 같은 볼륨이면 rename, 아니면 copy → flush → 해시 검증 → 원본 제거.
/// 반환값: 이동된 파일의 SHA-256.
pub fn safe_move(src: &Path, dst: &Path) -> AppResult<String> {
    if dst.exists() {
        return Err(AppError::new("ALREADY_EXISTS", "대상 위치에 같은 이름의 파일이 있어 이동하지 않았습니다"));
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    let hash = sha256_file(src)?;
    match fs::rename(src, dst) {
        Ok(()) => Ok(hash),
        Err(_) => {
            // 볼륨이 다르면 rename이 실패한다. 복사 후 검증한다.
            let tmp = dst.with_extension("autodesk-part");
            {
                let mut input = fs::File::open(src)?;
                let mut out = fs::File::create(&tmp)?;
                std::io::copy(&mut input, &mut out)?;
                out.flush()?;
                out.sync_all()?;
            }
            if sha256_file(&tmp)? != hash {
                let _ = fs::remove_file(&tmp);
                return Err(AppError::new("VERIFY_FAILED", "복사본 검증에 실패해 원본을 그대로 두었습니다"));
            }
            fs::rename(&tmp, dst)?;
            fs::remove_file(src)?;
            Ok(hash)
        }
    }
}

/// 임시 파일에 쓴 뒤 원자적으로 최종 이름으로 바꾼다.
pub fn atomic_copy(src: &Path, dst: &Path) -> AppResult<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = dst.with_extension("tmp-snapshot");
    {
        let mut input = fs::File::open(src)?;
        let mut out = fs::File::create(&tmp)?;
        std::io::copy(&mut input, &mut out)?;
        out.flush()?;
        out.sync_all()?;
    }
    fs::rename(&tmp, dst)?;
    Ok(())
}

/// `child`가 `root` 아래에 있는지(정규화 경로 기준) 확인한다. symlink 탈출을 막기 위해 canonicalize한다.
pub fn is_within(root: &Path, child: &Path) -> bool {
    match (dunce_canon(root), dunce_canon(child)) {
        (Some(r), Some(c)) => c.starts_with(r),
        _ => false,
    }
}

/// 존재하지 않을 수 있는 경로도 다룰 수 있도록, 존재하는 가장 가까운 상위까지 canonicalize한다.
pub fn dunce_canon(p: &Path) -> Option<PathBuf> {
    if let Ok(c) = fs::canonicalize(p) {
        return Some(strip_verbatim(c));
    }
    let parent = p.parent()?;
    let name = p.file_name()?;
    Some(dunce_canon(parent)?.join(name))
}

fn strip_verbatim(p: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let s = p.to_string_lossy();
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            if !rest.starts_with("UNC\\") {
                return PathBuf::from(rest);
            }
        }
    }
    p
}

/// 감시 폴더로 등록하면 안 되는 위치(시스템 루트, 앱 데이터 폴더 자체/상위)를 걸러낸다.
pub fn validate_watch_root(path: &Path, app_data: &Path) -> AppResult<PathBuf> {
    let canon = dunce_canon(path).ok_or_else(|| AppError::invalid("폴더 경로를 확인할 수 없습니다"))?;
    if !canon.is_dir() {
        return Err(AppError::invalid("폴더가 아니거나 존재하지 않습니다"));
    }
    if canon.parent().is_none() || canon.components().count() <= 2 {
        return Err(AppError::invalid("드라이브 루트나 시스템 최상위 폴더는 감시할 수 없습니다"));
    }
    if let Some(app) = dunce_canon(app_data) {
        if canon.starts_with(&app) || app.starts_with(&canon) {
            return Err(AppError::invalid("AutoDesk 데이터 폴더와 겹치는 위치는 감시할 수 없습니다"));
        }
    }
    let lower = canon.to_string_lossy().to_lowercase();
    for sys in ["c:\\windows", "c:\\program files", "/system", "/library", "/usr", "/bin", "/etc"] {
        if lower.starts_with(sys) {
            return Err(AppError::invalid("시스템 폴더는 감시할 수 없습니다"));
        }
    }
    Ok(canon)
}

/// 감시할 로그 '파일' 검증: 존재하는 일반 파일, 앱 데이터 폴더 밖.
pub fn validate_log_file(path: &Path, app_data: &Path) -> AppResult<PathBuf> {
    let canon = dunce_canon(path).ok_or_else(|| AppError::invalid("파일 경로를 확인할 수 없습니다"))?;
    if !canon.is_file() {
        return Err(AppError::invalid("로그 파일이 아니거나 존재하지 않습니다"));
    }
    if let Some(app) = dunce_canon(app_data) {
        if canon.starts_with(&app) {
            return Err(AppError::invalid("AutoDesk 자체 로그는 감시할 수 없습니다"));
        }
    }
    if canon.parent().map(|p| p.components().count() < 2).unwrap_or(true) {
        return Err(AppError::invalid("드라이브 루트의 파일은 감시할 수 없습니다"));
    }
    Ok(canon)
}

/// 단순 glob(`*` 와일드카드, 대소문자 무시). `dir/**` 형태는 경로 구성 요소 일치로 처리한다.
pub fn matches_pattern(path: &Path, pattern: &str) -> bool {
    let pat = pattern.to_lowercase();
    if let Some(dir) = pat.strip_suffix("/**") {
        return path
            .components()
            .any(|c| c.as_os_str().to_string_lossy().to_lowercase() == dir);
    }
    let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    wildcard(&pat, &name)
}

fn wildcard(pat: &str, s: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let t: Vec<char> = s.chars().collect();
    let (mut pi, mut ti, mut star, mut mark) = (0usize, 0usize, None::<usize>, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == t[ti] || p[pi] == '?') {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(rd) = fs::read_dir(path) {
        for e in rd.flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => total += dir_size(&e.path()),
                Ok(t) if t.is_file() => total += e.metadata().map(|m| m.len()).unwrap_or(0),
                _ => {}
            }
        }
    }
    total
}

/// 화면/이력에 표시할 때 긴 경로를 축약한다(마지막 두 구성 요소만).
pub fn short_path(p: &Path) -> String {
    let parts: Vec<_> = p.components().rev().take(2).map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    let mut parts = parts;
    parts.reverse();
    let joined = parts.join(std::path::MAIN_SEPARATOR_STR);
    if p.components().count() > 2 {
        format!("…{}{}", std::path::MAIN_SEPARATOR, joined)
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_patterns() {
        assert!(matches_pattern(Path::new("a/b/file.TMP"), "*.tmp"));
        assert!(matches_pattern(Path::new("a/~$doc.docx"), "~$*"));
        assert!(matches_pattern(Path::new("p/node_modules/x/y.js"), "node_modules/**"));
        assert!(!matches_pattern(Path::new("p/src/y.js"), "node_modules/**"));
        assert!(matches_pattern(Path::new(".DS_Store"), ".ds_store"));
        assert!(!matches_pattern(Path::new("report.txt"), "*.tmp"));
    }

    #[test]
    fn unique_path_avoids_collision() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("a.txt"), "1").unwrap();
        fs::write(d.path().join("a (2).txt"), "2").unwrap();
        assert_eq!(unique_path(d.path(), "a.txt"), d.path().join("a (3).txt"));
        assert_eq!(unique_path(d.path(), "b.txt"), d.path().join("b.txt"));
    }

    #[test]
    fn safe_move_never_overwrites() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("s.txt");
        let dst = d.path().join("sub/d.txt");
        fs::write(&src, "hello").unwrap();
        let h = safe_move(&src, &dst).unwrap();
        assert_eq!(h, sha256_str("hello"));
        assert!(!src.exists());
        fs::write(&src, "again").unwrap();
        assert!(safe_move(&src, &dst).is_err());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "hello");
    }

    #[test]
    fn path_traversal_checks() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join("in")).unwrap();
        assert!(is_within(d.path(), &d.path().join("in")));
        assert!(!is_within(&d.path().join("in"), d.path()));
        assert!(!is_within(&d.path().join("in"), &d.path().join("in/../other")));
    }

    #[test]
    fn watch_root_rejects_app_data_overlap() {
        let d = tempfile::tempdir().unwrap();
        let app = d.path().join("app");
        let user = d.path().join("docs");
        fs::create_dir_all(&app).unwrap();
        fs::create_dir_all(&user).unwrap();
        assert!(validate_watch_root(&user, &app).is_ok());
        assert!(validate_watch_root(&app, &app).is_err());
        assert!(validate_watch_root(d.path(), &app).is_err());
    }
}
