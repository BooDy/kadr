// crates/kadr-core/src/subtitles/transcoder.rs

/// Normalizes a SubRip timestamp to WebVTT format (replacing comma with period
/// and ensuring 3 millisecond digits).
fn normalize_timestamp(ts: &str) -> String {
    let ts = ts.replace(',', ".");
    if let Some((main, ms)) = ts.split_once('.') {
        let ms_padded = match ms.len() {
            0 => "000".to_string(),
            1 => format!("{}00", ms),
            2 => format!("{}0", ms),
            3 => ms.to_string(),
            _ => ms.chars().take(3).collect::<String>(),
        };

        let parts: Vec<&str> = main.split(':').collect();
        if parts.len() == 3 {
            let h = parts[0];
            let m = parts[1];
            let s = parts[2];
            let h_padded = if h.len() == 1 { format!("0{}", h) } else { h.to_string() };
            let m_padded = if m.len() == 1 { format!("0{}", m) } else { m.to_string() };
            let s_padded = if s.len() == 1 { format!("0{}", s) } else { s.to_string() };
            format!("{}:{}:{}.{}", h_padded, m_padded, s_padded, ms_padded)
        } else if parts.len() == 2 {
            let m = parts[0];
            let s = parts[1];
            let m_padded = if m.len() == 1 { format!("0{}", m) } else { m.to_string() };
            let s_padded = if s.len() == 1 { format!("0{}", s) } else { s.to_string() };
            format!("{}:{}.{}", m_padded, s_padded, ms_padded)
        } else {
            format!("{}.{}", main, ms_padded)
        }
    } else {
        format!("{}.000", ts)
    }
}

/// Normalizes timing line (e.g. `00:01:23,456 --> 00:01:25,789`) to WebVTT timing.
fn normalize_timing_line(line: &str) -> Option<String> {
    let parts: Vec<&str> = line.split("-->").collect();
    if parts.len() != 2 {
        return None;
    }

    let start_str = parts[0].trim();
    let right_str = parts[1].trim();

    let mut right_tokens = right_str.split_whitespace();
    let end_str = right_tokens.next()?;
    let settings: Vec<&str> = right_tokens
        .filter(|s| {
            !s.starts_with("X1:")
                && !s.starts_with("X2:")
                && !s.starts_with("Y1:")
                && !s.starts_with("Y2:")
        })
        .collect();

    let norm_start = normalize_timestamp(start_str);
    let norm_end = normalize_timestamp(end_str);

    if settings.is_empty() {
        Some(format!("{} --> {}", norm_start, norm_end))
    } else {
        Some(format!("{} --> {} {}", norm_start, norm_end, settings.join(" ")))
    }
}

/// Cleans cue text: preserves supported WebVTT tags (`<i>`, `<b>`, `<u>`, etc.)
/// while stripping unsupported tags (`<font>`, ASS `{\an8}`, etc.).
fn clean_cue_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' {
            let mut temp = String::new();
            let mut found_close = false;
            for next_ch in chars.by_ref() {
                if next_ch == '}' {
                    found_close = true;
                    break;
                }
                temp.push(next_ch);
            }
            if !found_close || temp.contains('\n') || temp.contains('{') || !temp.starts_with('\\') {
                result.push('{');
                result.push_str(&temp);
                if found_close {
                    result.push('}');
                }
            }
        } else if ch == '<' {
            let mut tag_content = String::new();
            let mut found_close = false;
            for next_ch in chars.by_ref() {
                if next_ch == '>' {
                    found_close = true;
                    break;
                }
                tag_content.push(next_ch);
            }

            if found_close && !tag_content.contains('<') && !tag_content.contains('\n') {
                let trimmed_tag = tag_content.trim();
                let tag_name = trimmed_tag
                    .strip_prefix('/')
                    .unwrap_or(trimmed_tag)
                    .split(|c: char| c.is_whitespace() || c == '.')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();

                match tag_name.as_str() {
                    "i" | "b" | "u" | "v" | "c" | "ruby" | "rt" | "lang" => {
                        result.push('<');
                        result.push_str(&tag_content);
                        result.push('>');
                    }
                    _ => {
                        // Strip unsupported tag
                    }
                }
            } else {
                result.push('<');
                result.push_str(&tag_content);
                if found_close {
                    result.push('>');
                }
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Converts SubRip (SRT) format subtitle content into valid WebVTT format.
///
/// Features:
/// - Strips UTF-8 BOM (`\u{FEFF}`).
/// - Normalizes line endings (`\r\n` -> `\n`).
/// - Emits `WEBVTT\n` for empty/whitespace input, or `WEBVTT\n\n` followed by cues.
/// - Converts timestamps from `00:01:23,456` to `00:01:23.456`.
/// - Preserves cue identifiers / sequence numbers when present.
/// - Preserves styling tags (`<i>`, `<b>`, `<u>`) and strips unsupported tags (e.g. `<font>`).
pub fn srt_to_webvtt(srt: &str) -> String {
    let srt = srt.strip_prefix('\u{FEFF}').unwrap_or(srt);
    if srt.trim().is_empty() {
        return "WEBVTT\n".to_string();
    }

    let normalized = srt.replace("\r\n", "\n").replace('\r', "\n");

    let mut blocks: Vec<Vec<&str>> = Vec::new();
    let mut current_block: Vec<&str> = Vec::new();

    for line in normalized.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !current_block.is_empty() {
                blocks.push(std::mem::take(&mut current_block));
            }
        } else {
            current_block.push(line);
        }
    }
    if !current_block.is_empty() {
        blocks.push(current_block);
    }

    if blocks.is_empty() {
        return "WEBVTT\n".to_string();
    }

    let mut output = String::from("WEBVTT\n\n");
    let mut first_cue = true;

    for block in blocks {
        let timing_idx = match block.iter().position(|l| l.contains("-->")) {
            Some(idx) => idx,
            None => continue, // Skip blocks without timing lines
        };

        let timing_line = match normalize_timing_line(block[timing_idx]) {
            Some(line) => line,
            None => continue,
        };

        let id = if timing_idx > 0 {
            Some(block[timing_idx - 1].trim())
        } else {
            None
        };

        let cue_lines: Vec<String> = block[timing_idx + 1..]
            .iter()
            .map(|l| clean_cue_text(l.trim_end()))
            .collect();

        if !first_cue {
            output.push('\n');
        }
        first_cue = false;

        if let Some(id_str) = id {
            output.push_str(id_str);
            output.push('\n');
        }

        output.push_str(&timing_line);
        output.push('\n');

        for line in cue_lines {
            output.push_str(&line);
            output.push('\n');
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_timestamp_variants() {
        assert_eq!(normalize_timestamp("00:01:23,456"), "00:01:23.456");
        assert_eq!(normalize_timestamp("1:01:23,45"), "01:01:23.450");
        assert_eq!(normalize_timestamp("1:23,4"), "01:23.400");
        assert_eq!(normalize_timestamp("00:01:23"), "00:01:23.000");
    }

    #[test]
    fn test_normalize_timing_line_with_coordinates() {
        let line = "00:01:23,456 --> 00:01:25,789 X1:076 X2:332 Y1:039 Y2:076";
        let norm = normalize_timing_line(line).expect("valid timing line");
        assert_eq!(norm, "00:01:23.456 --> 00:01:25.789");
    }

    #[test]
    fn test_clean_cue_text_unmatched_or_complex() {
        assert_eq!(clean_cue_text("A < B and C > D"), "A < B and C > D");
        assert_eq!(clean_cue_text("<i>Hello</i> <font color=\"red\">World</font>"), "<i>Hello</i> World");
        assert_eq!(clean_cue_text("{\\an8}{\\b1}Bold and top"), "Bold and top");
    }

    #[test]
    fn test_skip_malformed_blocks() {
        let srt = "Invalid block without timing\nJust some random text\n\n1\n00:00:01,000 --> 00:00:02,000\nValid cue\n";
        let vtt = srt_to_webvtt(srt);
        assert_eq!(vtt, "WEBVTT\n\n1\n00:00:01.000 --> 00:00:02.000\nValid cue\n");
    }
}
