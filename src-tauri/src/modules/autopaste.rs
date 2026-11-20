//! 클립보드 정리: 전역 단축키를 눌렀을 때만 클립보드 텍스트를 읽어 로컬 규칙으로 정리한다.
//! 클립보드 원문/정리문은 DB·로그·이력에 저장하지 않는다(건수만 기록).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoPasteConfig {
    pub shortcut: String,
    pub trim_trailing: bool,
    pub remove_zero_width: bool,
    pub nbsp_to_space: bool,
    pub collapse_blank_lines: bool,
    pub max_blank_lines: usize,
    pub trim_ends: bool,
    pub tabs_to_spaces: bool,
    pub tab_width: usize,
    pub strip_html: bool,
    /// keep | lf | crlf
    pub line_ending: String,
    /// 코드/로그로 보이면 일반 텍스트 정리를 건너뛴다.
    pub auto_code_mode: bool,
    /// 정리 결과 알림에 건수만 표시(텍스트 미표시). 항상 텍스트는 표시하지 않으며 이 값은 알림 자체 여부.
    pub notify: bool,
    // ---- 선택 기능: Claude Code로 다듬기 ----
    pub ai_enabled: bool,
    pub ai_consent: bool,
    pub ai_shortcut: String,
    pub ai_model: String,
}

impl Default for AutoPasteConfig {
    fn default() -> Self {
        Self {
            shortcut: "CommandOrControl+Alt+Shift+KeyV".into(),
            trim_trailing: true,
            remove_zero_width: true,
            nbsp_to_space: true,
            collapse_blank_lines: true,
            max_blank_lines: 1,
            trim_ends: true,
            tabs_to_spaces: false,
            tab_width: 4,
            strip_html: false,
            line_ending: "keep".into(),
            auto_code_mode: true,
            notify: true,
            ai_enabled: false,
            ai_consent: false,
            ai_shortcut: "CommandOrControl+Alt+Shift+KeyB".into(),
            ai_model: "haiku".into(),
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanStats {
    pub zero_width_removed: usize,
    pub nbsp_replaced: usize,
    pub trailing_trimmed: usize,
    pub blank_lines_removed: usize,
    pub tabs_converted: usize,
    pub html_tags_removed: usize,
    pub code_mode: bool,
}

impl CleanStats {
    pub fn total(&self) -> usize {
        self.zero_width_removed
            + self.nbsp_replaced
            + self.trailing_trimmed
            + self.blank_lines_removed
            + self.tabs_converted
            + self.html_tags_removed
    }
}

/// 코드/로그로 보이는지 대략 판단한다(들여쓰기 줄 비율, 코드 펜스, 중괄호/세미콜론 줄).
pub fn looks_like_code(text: &str) -> bool {
    if text.contains("```") {
        return true;
    }
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() < 3 {
        return false;
    }
    let indented = lines.iter().filter(|l| l.starts_with("    ") || l.starts_with('\t')).count();
    let codey = lines
        .iter()
        .filter(|l| {
            let t = l.trim_end();
            t.ends_with('{') || t.ends_with('}') || t.ends_with(';') || t.starts_with("at ") || t.contains(" ERROR ")
        })
        .count();
    indented * 10 >= lines.len() * 3 || codey * 10 >= lines.len() * 4
}

fn strip_html_tags(s: &str, stats: &mut CleanStats) -> String {
    // 텍스트로 읽힌 HTML일 때만(태그 모양이 있을 때) 동작한다.
    if !(s.contains('<') && s.contains('>') && (s.contains("</") || s.contains("<br") || s.contains("<p"))) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    let mut tag = String::new();
    for ch in s.chars() {
        match (in_tag, ch) {
            (false, '<') => {
                in_tag = true;
                tag.clear();
            }
            (true, '>') => {
                in_tag = false;
                stats.html_tags_removed += 1;
                let t = tag.trim_start_matches('/').to_ascii_lowercase();
                if t.starts_with("br") || t.starts_with("p") || t.starts_with("div") || t.starts_with("li") {
                    out.push('\n');
                }
            }
            (true, c) => tag.push(c),
            (false, c) => out.push(c),
        }
    }
    if in_tag {
        // 닫히지 않은 '<'는 태그가 아니었다.
        out.push('<');
        out.push_str(&tag);
    }
    out.replace("&nbsp;", "\u{00A0}")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// 텍스트를 정리한다. ZWJ(U+200D)·variation selector·ZWNJ는 보존한다.
pub fn clean_text(input: &str, cfg: &AutoPasteConfig) -> (String, CleanStats) {
    let mut stats = CleanStats::default();
    let uses_crlf = input.contains("\r\n");
    let mut s = input.replace("\r\n", "\n");

    let code_mode = cfg.auto_code_mode && looks_like_code(&s);
    stats.code_mode = code_mode;

    if cfg.strip_html && !code_mode {
        s = strip_html_tags(&s, &mut stats);
    }

    if cfg.remove_zero_width {
        let before = s.chars().count();
        s = s.chars().filter(|&c| c != '\u{200B}' && c != '\u{FEFF}').collect();
        stats.zero_width_removed = before - s.chars().count();
    }
    if cfg.nbsp_to_space {
        stats.nbsp_replaced = s.matches('\u{00A0}').count();
        s = s.replace('\u{00A0}', " ");
    }

    if !code_mode {
        let mut lines: Vec<String> = Vec::new();
        for line in s.split('\n') {
            let mut l = line.to_string();
            if cfg.tabs_to_spaces && l.contains('\t') {
                stats.tabs_converted += l.matches('\t').count();
                l = l.replace('\t', &" ".repeat(cfg.tab_width.clamp(1, 8)));
            }
            if cfg.trim_trailing {
                let t = l.trim_end_matches([' ', '\t']).to_string();
                if t.len() != l.len() {
                    stats.trailing_trimmed += 1;
                }
                l = t;
            }
            lines.push(l);
        }
        if cfg.collapse_blank_lines {
            let mut out: Vec<String> = Vec::with_capacity(lines.len());
            let mut blank_run = 0usize;
            for l in lines {
                if l.trim().is_empty() {
                    blank_run += 1;
                    if blank_run > cfg.max_blank_lines {
                        stats.blank_lines_removed += 1;
                        continue;
                    }
                    out.push(String::new());
                } else {
                    blank_run = 0;
                    out.push(l);
                }
            }
            lines = out;
        }
        s = lines.join("\n");
        if cfg.trim_ends {
            let t = s.trim_matches(|c: char| c == '\n' || c == ' ' || c == '\t').to_string();
            if t.len() != s.len() {
                stats.trailing_trimmed += 1;
            }
            s = t;
        }
    }

    let want_crlf = match cfg.line_ending.as_str() {
        "crlf" => true,
        "lf" => false,
        _ => uses_crlf,
    };
    if want_crlf {
        s = s.replace('\n', "\r\n");
    }
    (s, stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> AutoPasteConfig {
        AutoPasteConfig::default()
    }

    #[test]
    fn korean_and_english_trailing_spaces() {
        let (out, st) = clean_text("안녕하세요   \nHello world  \n", &cfg());
        assert_eq!(out, "안녕하세요\nHello world");
        assert!(st.trailing_trimmed >= 2);
    }

    #[test]
    fn nbsp_and_zero_width() {
        let (out, st) = clean_text("a\u{00A0}b\u{200B}c", &cfg());
        assert_eq!(out, "a bc");
        assert_eq!(st.nbsp_replaced, 1);
        assert_eq!(st.zero_width_removed, 1);
    }

    #[test]
    fn emoji_zwj_sequences_preserved() {
        let family = "👨\u{200D}👩\u{200D}👧 ❤\u{FE0F}";
        let (out, _) = clean_text(family, &cfg());
        assert_eq!(out, family);
    }

    #[test]
    fn collapses_blank_lines() {
        let (out, st) = clean_text("a\n\n\n\n\nb", &cfg());
        assert_eq!(out, "a\n\nb");
        assert_eq!(st.blank_lines_removed, 3);
    }

    #[test]
    fn whitespace_only_lines_become_blank() {
        let (out, _) = clean_text("a\n   \n\t\nb", &cfg());
        assert_eq!(out, "a\n\nb");
    }

    #[test]
    fn code_indentation_preserved() {
        let code = "fn main() {\n    let x = 1;  \n\n\n\n    println!(\"{x}\");\n}";
        let (out, st) = clean_text(code, &cfg());
        assert!(st.code_mode);
        assert_eq!(out, code);
    }

    #[test]
    fn urls_and_paths_untouched() {
        let t = "https://example.com/a_b?x=1&y=2\nC:\\Users\\me\\file.txt\n/usr/local/bin";
        let (out, _) = clean_text(t, &cfg());
        assert_eq!(out, t);
    }

    #[test]
    fn crlf_is_kept_by_default_and_convertible() {
        let (out, _) = clean_text("a  \r\nb", &cfg());
        assert_eq!(out, "a\r\nb");
        let mut c = cfg();
        c.line_ending = "lf".into();
        assert_eq!(clean_text("a\r\nb", &c).0, "a\nb");
    }

    #[test]
    fn html_strip_optional() {
        let mut c = cfg();
        c.strip_html = true;
        let (out, st) = clean_text("<p>Hi &amp; bye</p>", &c);
        assert_eq!(out, "Hi & bye");
        assert!(st.html_tags_removed >= 2);
        // 비교 기호만 있는 일반 텍스트는 건드리지 않는다.
        assert_eq!(clean_text("1 < 2 > 0", &c).0, "1 < 2 > 0");
    }

    #[test]
    fn empty_and_clean_text_unchanged() {
        assert_eq!(clean_text("", &cfg()).0, "");
        let (out, st) = clean_text("already clean", &cfg());
        assert_eq!(out, "already clean");
        assert_eq!(st.total(), 0);
    }
}
