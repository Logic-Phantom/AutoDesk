//! 도구 공용 파일 입출력: 텍스트 읽기(DOCX는 본문 텍스트 추출), 결과 저장.
//! 경로는 사용자가 OS 대화상자로 고른 값만 들어온다. 확장자·크기를 제한한다.

use crate::error::{AppError, AppResult};
use base64::Engine;
use std::io::Read;
use std::path::Path;

const MAX_TEXT_BYTES: u64 = 20 * 1024 * 1024;

fn ext(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

/// DOCX의 word/document.xml에서 문단 텍스트만 뽑는다(서식 무시).
pub fn docx_text(path: &Path) -> AppResult<String> {
    let f = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(f).map_err(|e| AppError::new("DOCX_INVALID", format!("DOCX 파일을 열 수 없습니다: {e}")))?;
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .map_err(|_| AppError::new("DOCX_INVALID", "DOCX 본문(word/document.xml)이 없습니다"))?
        .take(MAX_TEXT_BYTES)
        .read_to_string(&mut xml)?;
    Ok(docx_xml_to_text(&xml))
}

/// `<w:p>` = 문단(줄바꿈), `<w:t>` = 텍스트, `<w:tab/>` = 탭, `<w:br/>` = 줄바꿈.
pub fn docx_xml_to_text(xml: &str) -> String {
    let mut out = String::new();
    let mut i = 0;
    let b = xml.as_bytes();
    while i < b.len() {
        if b[i] == b'<' {
            let end = match xml[i..].find('>') {
                Some(e) => i + e,
                None => break,
            };
            let tag = &xml[i + 1..end];
            let name = tag.split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("");
            match name {
                "w:t" if !tag.ends_with('/') => {
                    let close = xml[end + 1..].find("</w:t>").map(|c| end + 1 + c).unwrap_or(b.len());
                    out.push_str(&unescape(&xml[end + 1..close]));
                    i = close + "</w:t>".len().min(b.len() - close);
                    continue;
                }
                "w:tab" => out.push('\t'),
                "w:br" | "w:cr" => out.push('\n'),
                _ if tag.starts_with("/w:p") && (tag.len() == 4 || tag.as_bytes()[4] == b'>') => out.push('\n'),
                _ => {}
            }
            i = end + 1;
        } else {
            i += 1;
        }
    }
    out.trim_end().to_string() + "\n"
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// 텍스트 파일 읽기. UTF-8이 아니면 EUC-KR로 해석. DOCX는 본문만 추출.
pub fn read_text(path: &Path) -> AppResult<String> {
    let meta = std::fs::metadata(path)?;
    if !meta.is_file() {
        return Err(AppError::invalid("파일이 아닙니다"));
    }
    if meta.len() > MAX_TEXT_BYTES {
        return Err(AppError::invalid("20MB보다 큰 파일은 열 수 없습니다"));
    }
    if ext(path) == "docx" {
        return docx_text(path);
    }
    let bytes = std::fs::read(path)?;
    if bytes.iter().take(8000).filter(|&&b| b == 0).count() > 0 {
        return Err(AppError::invalid("텍스트 파일이 아닌 것 같습니다(바이너리)"));
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    Ok(crate::fsutil::decode_text(bytes))
}

const TEXT_EXT: &[&str] = &["txt", "md", "csv", "tsv", "json", "html", "htm"];
const BIN_EXT: &[&str] = &["png", "jpg", "jpeg"];

/// 결과 텍스트 저장. `bom`이면 UTF-8 BOM을 붙인다(엑셀에서 한글 CSV가 깨지지 않게).
pub fn save_text(path: &Path, contents: &str, bom: bool) -> AppResult<()> {
    if !TEXT_EXT.contains(&ext(path).as_str()) {
        return Err(AppError::invalid("저장할 수 없는 확장자입니다"));
    }
    let mut data = Vec::with_capacity(contents.len() + 3);
    if bom {
        data.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    data.extend_from_slice(contents.as_bytes());
    std::fs::write(path, data)?;
    Ok(())
}

pub fn save_base64(path: &Path, b64: &str) -> AppResult<()> {
    if !BIN_EXT.contains(&ext(path).as_str()) {
        return Err(AppError::invalid("저장할 수 없는 확장자입니다"));
    }
    let data = base64::engine::general_purpose::STANDARD
        .decode(b64.trim_start_matches("data:image/png;base64,").trim_start_matches("data:image/jpeg;base64,"))
        .map_err(|e| AppError::invalid(format!("이미지 데이터 오류: {e}")))?;
    std::fs::write(path, data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docx_xml_extraction() {
        let xml = r#"<w:document><w:body><w:p><w:r><w:t>제1조 (목적)</w:t></w:r></w:p><w:p><w:r><w:t xml:space="preserve">이 계약은 </w:t></w:r><w:r><w:t>A&amp;B</w:t></w:r><w:r><w:tab/><w:t>끝</w:t></w:r></w:p><w:p/></w:body></w:document>"#;
        assert_eq!(docx_xml_to_text(xml), "제1조 (목적)\n이 계약은 A&B\t끝\n");
    }

    #[test]
    fn save_rejects_other_extensions() {
        let d = tempfile::tempdir().unwrap();
        assert!(save_text(&d.path().join("x.exe"), "a", false).is_err());
        save_text(&d.path().join("x.csv"), "가,나", true).unwrap();
        let b = std::fs::read(d.path().join("x.csv")).unwrap();
        assert_eq!(&b[..3], &[0xEF, 0xBB, 0xBF]);
    }

    #[test]
    fn read_text_rejects_binary_and_handles_bom() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.bin"), [0u8, 1, 2, 0]).unwrap();
        assert!(read_text(&d.path().join("a.bin")).is_err());
        std::fs::write(d.path().join("b.txt"), b"\xEF\xBB\xBFhello").unwrap();
        assert_eq!(read_text(&d.path().join("b.txt")).unwrap(), "hello");
    }
}
