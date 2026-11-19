//! Download Butler: Downloads 폴더(최상위)에 새로 들어온 파일을 로컬 규칙으로 분류한다.
//! 자동 삭제·실행·덮어쓰기는 절대 하지 않으며, 모든 이동은 되돌릴 수 있다.

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DownloadsConfig {
    /// preview: 이동 제안만(승인 필요), auto: 확실한 분류는 자동 이동
    pub mode: String,
    /// OS 기본 Downloads 폴더를 감시할지
    pub watch_default_dir: bool,
    pub exclude_extensions: Vec<String>,
}

impl Default for DownloadsConfig {
    fn default() -> Self {
        Self { mode: "preview".into(), watch_default_dir: true, exclude_extensions: vec![] }
    }
}

/// 다운로드 중(미완료)일 때 브라우저가 쓰는 임시 확장자.
const PARTIAL_EXT: &[&str] = &["crdownload", "part", "partial", "download", "tmp", "opdownload", "!ut", "aria2", "autodesk-part"];

pub const CATEGORY_FOLDERS: &[&str] = &[
    "Documents/PDF",
    "Documents/Office",
    "Documents/Text",
    "Images",
    "Videos",
    "Audio",
    "Archives",
    "Installers",
];

/// `Documents/PDF` 같은 분류 이름을 OS 경로 구분자에 맞게 붙인다.
pub fn category_dir(base: &Path, category: &str) -> std::path::PathBuf {
    category.split('/').filter(|p| !p.is_empty() && *p != "..").fold(base.to_path_buf(), |acc, p| acc.join(p))
}

pub fn should_ignore(path: &Path, cfg: &DownloadsConfig) -> bool {
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return true,
    };
    if name.starts_with('.') || name.starts_with("~$") || name.eq_ignore_ascii_case("desktop.ini") || name.eq_ignore_ascii_case("thumbs.db") {
        return true;
    }
    let ext = ext_lower(path);
    if PARTIAL_EXT.contains(&ext.as_str()) {
        return true;
    }
    cfg.exclude_extensions.iter().any(|e| e.trim_start_matches('.').eq_ignore_ascii_case(&ext))
}

fn ext_lower(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

pub fn classify_extension(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "pdf" => "Documents/PDF",
        "doc" | "docx" | "odt" | "rtf" | "hwp" | "hwpx" | "xls" | "xlsx" | "ods" | "csv" | "ppt" | "pptx" | "odp" => "Documents/Office",
        "txt" | "md" | "log" | "json" | "xml" | "yaml" | "yml" => "Documents/Text",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "heic" | "tif" | "tiff" | "ico" => "Images",
        "mp4" | "mov" | "mkv" | "webm" | "avi" | "wmv" | "m4v" => "Videos",
        "mp3" | "wav" | "m4a" | "flac" | "ogg" | "aac" => "Audio",
        "zip" | "7z" | "rar" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "alz" | "egg" => "Archives",
        "exe" | "msi" | "dmg" | "pkg" | "appx" | "msix" => "Installers",
        _ => return None,
    })
}

/// 확장자가 없거나 모를 때 파일 앞부분 시그니처로 판별한다(파일은 실행하지 않는다).
pub fn classify_signature(path: &Path) -> Option<&'static str> {
    let mut buf = [0u8; 8];
    let mut f = std::fs::File::open(path).ok()?;
    let n = f.read(&mut buf).ok()?;
    let b = &buf[..n];
    if b.starts_with(b"%PDF") {
        Some("Documents/PDF")
    } else if b.starts_with(&[0x89, b'P', b'N', b'G']) || b.starts_with(&[0xFF, 0xD8, 0xFF]) || b.starts_with(b"GIF8") {
        Some("Images")
    } else if b.starts_with(b"PK\x03\x04") || b.starts_with(b"7z\xBC\xAF") || b.starts_with(b"Rar!") {
        Some("Archives")
    } else if b.starts_with(b"MZ") {
        Some("Installers")
    } else {
        None
    }
}

/// 분류 결과와 확신 여부. 확장자와 시그니처가 충돌하면(예: .pdf인데 실행 파일) 확신하지 않는다.
pub fn classify(path: &Path) -> (Option<&'static str>, bool) {
    let by_ext = classify_extension(&ext_lower(path));
    let by_sig = classify_signature(path);
    match (by_ext, by_sig) {
        (Some(e), Some("Installers")) if e != "Installers" => (Some("Installers"), false),
        (Some(e), _) => (Some(e), true),
        (None, Some(s)) => (Some(s), false),
        (None, None) => (None, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn category_dir_uses_native_separators() {
        let p = category_dir(Path::new("base"), "Documents/PDF");
        assert_eq!(p, Path::new("base").join("Documents").join("PDF"));
        assert_eq!(category_dir(Path::new("b"), "../x"), Path::new("b").join("x"));
    }

    #[test]
    fn extension_rules() {
        assert_eq!(classify_extension("pdf"), Some("Documents/PDF"));
        assert_eq!(classify_extension("xlsx"), Some("Documents/Office"));
        assert_eq!(classify_extension("png"), Some("Images"));
        assert_eq!(classify_extension("exe"), Some("Installers"));
        assert_eq!(classify_extension("weird"), None);
    }

    #[test]
    fn ignores_partial_and_hidden() {
        let c = DownloadsConfig::default();
        assert!(should_ignore(Path::new("a.pdf.crdownload"), &c));
        assert!(should_ignore(Path::new(".hidden"), &c));
        assert!(should_ignore(Path::new("x.part"), &c));
        assert!(!should_ignore(Path::new("a.pdf"), &c));
        let c2 = DownloadsConfig { exclude_extensions: vec![".pdf".into()], ..Default::default() };
        assert!(should_ignore(Path::new("a.pdf"), &c2));
    }

    #[test]
    fn signature_detection_and_mismatch() {
        let d = tempfile::tempdir().unwrap();
        let noext = d.path().join("file");
        fs::write(&noext, b"%PDF-1.7 ...").unwrap();
        assert_eq!(classify(&noext), (Some("Documents/PDF"), false));
        let fake = d.path().join("invoice.pdf");
        fs::write(&fake, b"MZ\x90\x00").unwrap();
        assert_eq!(classify(&fake), (Some("Installers"), false));
        let real = d.path().join("ok.pdf");
        fs::write(&real, b"%PDF").unwrap();
        assert_eq!(classify(&real), (Some("Documents/PDF"), true));
        let unknown = d.path().join("blob.bin2");
        fs::write(&unknown, b"\x00\x01").unwrap();
        assert_eq!(classify(&unknown), (None, false));
    }
}
