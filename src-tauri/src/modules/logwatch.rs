//! LogWatcher: 사용자가 고른 로그 파일에 새로 추가된 줄만 읽어 ERROR/Exception 등을 감지한다.
//! - 같은 오류는 지문(fingerprint)으로 묶어 횟수·최초/최근 시각만 늘린다(알림 폭주 방지).
//! - 파일이 작아지면(로테이션/초기화) 처음부터 다시 읽는다.
//! - 로그는 로컬에서만 분석하며 외부로 보내지 않는다.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LogWatchConfig {
    pub error_patterns: Vec<String>,
    pub warn_patterns: Vec<String>,
    pub case_sensitive: bool,
    /// WARN도 알림을 보낼지(기본: 오류만 알림, WARN은 기록만)
    pub notify_warn: bool,
    /// 같은 오류를 다시 알리기까지 기다리는 시간(분)
    pub renotify_minutes: i64,
    /// 오류 앞쪽에 함께 보관할 줄 수
    pub context_lines: usize,
}

impl Default for LogWatchConfig {
    fn default() -> Self {
        Self {
            error_patterns: ["ERROR", "FATAL", "SEVERE", "Exception", "[ERR]"].iter().map(|s| s.to_string()).collect(),
            warn_patterns: ["WARN", "WARNING"].iter().map(|s| s.to_string()).collect(),
            case_sensitive: true,
            notify_warn: false,
            renotify_minutes: 30,
            context_lines: 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Level {
    Error,
    Warn,
}

impl Level {
    pub fn as_str(&self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Warn => "WARN",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Found {
    pub level: Level,
    pub line: String,
    pub context: String,
    pub fingerprint: String,
}

/// UTF-8이 아니면 EUC-KR(CP949)로 해석한다(국내 WAS 로그 대응).
pub fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
            let (s, _, had_errors) = encoding_rs::EUC_KR.decode(bytes);
            if had_errors {
                String::from_utf8_lossy(bytes).to_string()
            } else {
                s.to_string()
            }
        }
    }
}

/// 시간·숫자·16진수·따옴표 안 값 등 매번 달라지는 부분을 지워 같은 종류의 오류를 묶는다.
pub fn normalize(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut last_hash = false;
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            // 0x1f3a 같은 16진수도 하나로
            if c == '0' && matches!(chars.peek(), Some('x') | Some('X')) {
                chars.next();
                while matches!(chars.peek(), Some(h) if h.is_ascii_hexdigit()) {
                    chars.next();
                }
            }
            if !last_hash {
                out.push('#');
                last_hash = true;
            }
            continue;
        }
        last_hash = false;
        if c.is_whitespace() {
            if !out.ends_with(' ') {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    // UUID/해시처럼 숫자·영문이 섞인 긴 토큰은 숫자 치환 후에도 달라질 수 있으므로 길이를 제한한다.
    let trimmed = out.trim();
    trimmed.chars().take(200).collect()
}

fn matches_any(line: &str, patterns: &[String], case_sensitive: bool) -> bool {
    if case_sensitive {
        patterns.iter().any(|p| !p.is_empty() && line.contains(p.as_str()))
    } else {
        let l = line.to_lowercase();
        patterns.iter().any(|p| !p.is_empty() && l.contains(&p.to_lowercase()))
    }
}

/// 스택 트레이스 연속 줄(`at ...`, `Caused by:`, `... 12 more`, 들여쓰기 줄)은 새 오류로 세지 않고 앞 오류의 문맥으로 붙인다.
fn is_continuation(line: &str) -> bool {
    let t = line.trim_start();
    line.starts_with('\t')
        || line.starts_with("    ")
        || t.starts_with("at ")
        || t.starts_with("Caused by:")
        || (t.starts_with("...") && t.ends_with("more"))
}

/// 새로 추가된 텍스트(완결된 줄들)를 검사한다. `previous`는 직전 청크의 마지막 몇 줄(문맥용).
pub fn scan(text: &str, previous: &[String], cfg: &LogWatchConfig) -> Vec<Found> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim_end_matches('\r');
        if line.trim().is_empty() || is_continuation(line) {
            continue;
        }
        let level = if matches_any(line, &cfg.error_patterns, cfg.case_sensitive) {
            Level::Error
        } else if matches_any(line, &cfg.warn_patterns, cfg.case_sensitive) {
            Level::Warn
        } else {
            continue;
        };
        // 앞 문맥: 이 청크의 앞줄 + 부족하면 이전 청크 끝줄
        let n = cfg.context_lines.min(20);
        let mut before: Vec<String> = lines[..i].iter().rev().take(n).map(|s| s.trim_end_matches('\r').to_string()).collect();
        if before.len() < n {
            before.extend(previous.iter().rev().take(n - before.len()).cloned());
        }
        before.reverse();
        // 뒤 문맥: 바로 이어지는 스택 트레이스(최대 15줄)
        let after: Vec<String> = lines[i + 1..]
            .iter()
            .map(|s| s.trim_end_matches('\r'))
            .take_while(|s| is_continuation(s))
            .take(15)
            .map(|s| s.to_string())
            .collect();
        let mut context = before;
        context.push(format!("▶ {line}"));
        context.extend(after);
        let mut ctx = context.join("\n");
        if ctx.len() > 4000 {
            ctx = ctx.chars().take(4000).collect();
        }
        found.push(Found {
            fingerprint: crate::fsutil::sha256_str(&format!("{}|{}", level.as_str(), normalize(line))),
            level,
            line: line.chars().take(500).collect(),
            context: ctx,
        });
    }
    found
}

/// 마지막 개행까지만 '완결된 줄'로 본다. 반환: (완결 텍스트 바이트 수)
pub fn complete_len(bytes: &[u8]) -> usize {
    bytes.iter().rposition(|&b| b == b'\n').map(|i| i + 1).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> LogWatchConfig {
        LogWatchConfig::default()
    }

    #[test]
    fn detects_error_and_warn() {
        let t = "2026-11-19 10:00:01 INFO start\n2026-11-19 10:00:02 WARN slow query\n2026-11-19 10:00:03 ERROR db down\n";
        let f = scan(t, &[], &cfg());
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].level, Level::Warn);
        assert_eq!(f[1].level, Level::Error);
        assert!(f[1].context.contains("▶ 2026-11-19 10:00:03 ERROR db down"));
        assert!(f[1].context.contains("WARN slow query"));
    }

    #[test]
    fn same_error_different_time_has_same_fingerprint() {
        let a = scan("2026-11-19 10:00:03 ERROR timeout after 3000ms id=0x1f\n", &[], &cfg());
        let b = scan("2026-11-20 23:59:59 ERROR timeout after 5000ms id=0xabc\n", &[], &cfg());
        assert_eq!(a[0].fingerprint, b[0].fingerprint);
        let c = scan("2026-11-20 23:59:59 ERROR connection refused\n", &[], &cfg());
        assert_ne!(a[0].fingerprint, c[0].fingerprint);
    }

    #[test]
    fn stack_trace_is_context_not_new_issues() {
        let t = "SEVERE: Servlet.service() threw exception\njava.lang.NullPointerException: x\n\tat com.a.B.c(B.java:10)\n\tat com.a.D.e(D.java:20)\nCaused by: java.io.IOException: boom\n\t... 12 more\nINFO ok\n";
        let f = scan(t, &[], &cfg());
        // SEVERE 줄 + NullPointerException 줄(둘 다 독립 줄), Caused by는 문맥
        assert_eq!(f.len(), 2);
        assert!(f[1].context.contains("at com.a.B.c"));
        assert!(f[1].context.contains("Caused by: java.io.IOException"));
        assert!(!f[1].context.contains("INFO ok"));
    }

    #[test]
    fn case_insensitive_option() {
        let mut c = cfg();
        assert!(scan("an error happened\n", &[], &c).is_empty());
        c.case_sensitive = false;
        assert_eq!(scan("an error happened\n", &[], &c).len(), 1);
    }

    #[test]
    fn uses_previous_chunk_for_context() {
        let prev = vec!["line A".to_string(), "line B".to_string()];
        let f = scan("ERROR x\n", &prev, &cfg());
        assert!(f[0].context.starts_with("line A\nline B\n▶ ERROR x"));
    }

    #[test]
    fn decodes_euc_kr() {
        let (bytes, _, _) = encoding_rs::EUC_KR.encode("[ERROR] 데이터베이스 연결 실패\n");
        let s = decode(&bytes);
        assert!(s.contains("데이터베이스 연결 실패"));
        assert_eq!(decode("정상 UTF-8".as_bytes()), "정상 UTF-8");
    }

    #[test]
    fn partial_line_not_consumed() {
        assert_eq!(complete_len(b"a\nb\npartial"), 4);
        assert_eq!(complete_len(b"no newline"), 0);
        assert_eq!(complete_len(b"x\r\n"), 3);
    }
}
