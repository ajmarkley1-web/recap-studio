use base64::Engine;
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::Path;

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub fn new_id(prefix: &str) -> String {
    format!(
        "{prefix}_{}",
        uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
    )
}

pub fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_dash = true;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "untitled".into()
    } else {
        trimmed
    }
}

/// Natural sort so `page2.png` comes before `page10.png`.
pub fn natural_key(name: &str) -> Vec<NatChunk> {
    let mut chunks = Vec::new();
    let mut buf = String::new();
    let mut in_digits = false;
    for ch in name.chars() {
        let is_digit = ch.is_ascii_digit();
        if buf.is_empty() {
            in_digits = is_digit;
        } else if is_digit != in_digits {
            chunks.push(finish_chunk(&buf, in_digits));
            buf.clear();
            in_digits = is_digit;
        }
        buf.push(ch.to_ascii_lowercase());
    }
    if !buf.is_empty() {
        chunks.push(finish_chunk(&buf, in_digits));
    }
    chunks
}

fn finish_chunk(buf: &str, digits: bool) -> NatChunk {
    if digits {
        NatChunk::Num(buf.parse::<u64>().unwrap_or(u64::MAX))
    } else {
        NatChunk::Text(buf.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum NatChunk {
    Num(u64),
    Text(String),
}

// Deliberately not using extended `(?x)` mode: in it, `#` opens a comment even
// inside a character class, which silently truncates `[\s_\-#]`.
static CHAPTER_NUM: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:ch(?:apter|ap|\.)?|ep(?:isode)?|vol(?:ume)?)?[\s_\-#]*(\d{1,4})(?:[._-](\d{1,2}))?")
        .unwrap()
});

/// Pull a chapter number out of a folder or file name, e.g. `Chapter 12`, `ch_007`,
/// `Ep 5.5`. Returns `None` when nothing numeric is present.
pub fn parse_chapter_number(name: &str) -> Option<f32> {
    let caps = CHAPTER_NUM.captures(name)?;
    let whole: f32 = caps.get(1)?.as_str().parse().ok()?;
    match caps.get(2) {
        Some(frac) => {
            let f: f32 = frac.as_str().parse().ok()?;
            let scale = 10f32.powi(frac.as_str().len() as i32);
            Some(whole + f / scale)
        }
        None => Some(whole),
    }
}

pub const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff", "avif",
];

pub fn is_image_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Split prose into sentences. Deliberately simple: we only need this for
/// compliance checks and storyboard shot boundaries, not linguistics.
pub fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        current.push(ch);
        if matches!(ch, '.' | '!' | '?') {
            // Don't split on decimals or common abbreviations.
            let next = chars.get(i + 1).copied();
            let prev = if current.len() >= 2 {
                chars.get(i.wrapping_sub(1)).copied()
            } else {
                None
            };
            let numeric_context = prev.map(|c| c.is_ascii_digit()).unwrap_or(false)
                && next.map(|c| c.is_ascii_digit()).unwrap_or(false);
            if !numeric_context {
                // consume any trailing quotes/brackets that belong to this sentence
                while let Some(n) = chars.get(i + 1) {
                    if matches!(n, '"' | '\'' | ')' | ']' | '”' | '’') {
                        current.push(*n);
                        i += 1;
                    } else {
                        break;
                    }
                }
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    out.push(trimmed.to_string());
                }
                current.clear();
            }
        }
        i += 1;
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        out.push(trimmed.to_string());
    }
    out
}

pub fn word_count(text: &str) -> usize {
    text.split_whitespace().filter(|w| !w.is_empty()).count()
}

/// Strip markdown fences and any leading prose the model added before JSON.
pub fn extract_json_block(raw: &str) -> String {
    let trimmed = raw.trim();
    // ```json ... ```
    if let Some(start) = trimmed.find("```") {
        let after = &trimmed[start + 3..];
        let after = after.strip_prefix("json").unwrap_or(after);
        let after = after.strip_prefix("JSON").unwrap_or(after);
        if let Some(end) = after.find("```") {
            let inner = after[..end].trim();
            if !inner.is_empty() {
                return inner.to_string();
            }
        }
    }
    // First balanced { .. } or [ .. ] in the response.
    for (open, close) in [('{', '}'), ('[', ']')] {
        if let Some(start) = trimmed.find(open) {
            let bytes: Vec<char> = trimmed.chars().collect();
            let start_idx = trimmed[..start].chars().count();
            let mut depth = 0i32;
            let mut in_string = false;
            let mut escaped = false;
            for (i, ch) in bytes.iter().enumerate().skip(start_idx) {
                if in_string {
                    if escaped {
                        escaped = false;
                    } else if *ch == '\\' {
                        escaped = true;
                    } else if *ch == '"' {
                        in_string = false;
                    }
                    continue;
                }
                match *ch {
                    '"' => in_string = true,
                    c if c == open => depth += 1,
                    c if c == close => {
                        depth -= 1;
                        if depth == 0 {
                            return bytes[start_idx..=i].iter().collect();
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    trimmed.to_string()
}

/// Best-effort JSON parse that tolerates the usual model output quirks.
pub fn parse_json_lenient<T: serde::de::DeserializeOwned>(raw: &str) -> anyhow::Result<T> {
    let block = extract_json_block(raw);
    match serde_json::from_str::<T>(&block) {
        Ok(v) => Ok(v),
        Err(first) => {
            // Retry after removing trailing commas, a very common failure.
            let cleaned = Regex::new(r",(\s*[}\]])")
                .unwrap()
                .replace_all(&block, "$1")
                .to_string();
            serde_json::from_str::<T>(&cleaned).map_err(|second| {
                anyhow::anyhow!(
                    "could not parse model JSON ({first}); after cleanup: {second}. Raw start: {}",
                    block.chars().take(300).collect::<String>()
                )
            })
        }
    }
}

pub fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut s: String = text.chars().take(max_chars).collect();
    s.push_str("\n...[truncated]");
    s
}

/// Keep the END of a string rather than the start. Used to carry the tail of
/// the narration into the next pass, where the last thing written matters and
/// the first thing written does not.
pub fn truncate_tail(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let kept: String = text.chars().skip(count - max_chars).collect();
    // Start on a word boundary so the carried-over text does not open mid-word.
    let kept = match kept.find(char::is_whitespace) {
        Some(i) => kept[i..].trim_start().to_string(),
        None => kept,
    };
    format!("...{kept}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_sort_orders_numerically() {
        let mut names = vec!["page10.png", "page2.png", "page1.png"];
        names.sort_by_key(|n| natural_key(n));
        assert_eq!(names, vec!["page1.png", "page2.png", "page10.png"]);
    }

    #[test]
    fn chapter_numbers_parse() {
        assert_eq!(parse_chapter_number("Chapter 12"), Some(12.0));
        assert_eq!(parse_chapter_number("ch_007"), Some(7.0));
        assert_eq!(parse_chapter_number("Ep 5.5"), Some(5.5));
        assert_eq!(parse_chapter_number("Vol. 3"), Some(3.0));
        assert_eq!(parse_chapter_number("#42"), Some(42.0));
        assert_eq!(parse_chapter_number("012"), Some(12.0));
        assert_eq!(parse_chapter_number("prologue"), None);
    }

    #[test]
    fn json_block_survives_fences_and_chatter() {
        let raw = "Sure! Here you go:\n```json\n{\"a\": 1}\n```\nHope that helps.";
        assert_eq!(extract_json_block(raw), "{\"a\": 1}");
        let bare = "Here: {\"a\": {\"b\": 2}} done";
        assert_eq!(extract_json_block(bare), "{\"a\": {\"b\": 2}}");
    }

    #[test]
    fn truncate_tail_keeps_the_end() {
        assert_eq!(truncate_tail("abc", 10), "abc");
        let tail = truncate_tail("one two three four", 12);
        assert_eq!(tail, "...three four");
        // The cut lands inside a word, so the word it lands in is dropped.
        assert_eq!(truncate_tail("one two three four", 9), "...four");
    }

    #[test]
    fn sentences_split_without_breaking_decimals() {
        let s = split_sentences("He hit 3.5 meters. Then he fell! Did he? Yes.");
        assert_eq!(s.len(), 4);
        assert_eq!(s[0], "He hit 3.5 meters.");
    }
}
