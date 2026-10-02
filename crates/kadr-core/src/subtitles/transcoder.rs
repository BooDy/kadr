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
            let h_padded = if h.len() == 1 {
                format!("0{}", h)
            } else {
                h.to_string()
            };
            let m_padded = if m.len() == 1 {
                format!("0{}", m)
            } else {
                m.to_string()
            };
            let s_padded = if s.len() == 1 {
                format!("0{}", s)
            } else {
                s.to_string()
            };
            format!("{}:{}:{}.{}", h_padded, m_padded, s_padded, ms_padded)
        } else if parts.len() == 2 {
            let m = parts[0];
            let s = parts[1];
            let m_padded = if m.len() == 1 {
                format!("0{}", m)
            } else {
                m.to_string()
            };
            let s_padded = if s.len() == 1 {
                format!("0{}", s)
            } else {
                s.to_string()
            };
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
        Some(format!(
            "{} --> {} {}",
            norm_start,
            norm_end,
            settings.join(" ")
        ))
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
            if !found_close || temp.contains('\n') || temp.contains('{') || !temp.starts_with('\\')
            {
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

fn write_cue_block<W: std::io::Write>(
    writer: &mut W,
    block: &[String],
    has_written_header: &mut bool,
) -> std::io::Result<()> {
    let timing_idx = match block.iter().position(|l| l.contains("-->")) {
        Some(idx) => idx,
        None => return Ok(()), // Skip blocks without timing lines
    };

    let timing_line = match normalize_timing_line(&block[timing_idx]) {
        Some(line) => line,
        None => return Ok(()),
    };

    if !*has_written_header {
        writer.write_all(b"WEBVTT\n\n")?;
        *has_written_header = true;
    } else {
        writer.write_all(b"\n")?;
    }

    if timing_idx > 0 {
        let id_str = block[timing_idx - 1].trim();
        writer.write_all(id_str.as_bytes())?;
        writer.write_all(b"\n")?;
    }

    writer.write_all(timing_line.as_bytes())?;
    writer.write_all(b"\n")?;

    for line in &block[timing_idx + 1..] {
        let cleaned = clean_cue_text(line.trim_end());
        writer.write_all(cleaned.as_bytes())?;
        writer.write_all(b"\n")?;
    }

    Ok(())
}

/// Converts SubRip (SRT) format subtitle stream into valid WebVTT format directly
/// into a writer without buffering the whole file in memory.
///
/// Features:
/// - Handles UTF-8 BOM if present on the first line.
/// - Reads line by line into a reusable buffer without ever buffering the entire file.
/// - Groups into cue blocks (separated by empty lines).
/// - Emits `WEBVTT\n\n` header if cues exist (or `WEBVTT\n` if empty).
/// - Normalizes timing and cue text for each block and writes directly to `writer`.
pub fn srt_to_webvtt_stream<R: std::io::BufRead, W: std::io::Write>(
    mut reader: R,
    mut writer: W,
) -> std::io::Result<()> {
    let mut byte_buf = Vec::new();
    let mut current_block: Vec<String> = Vec::new();
    let mut is_first_line = true;
    let mut has_written_header = false;

    loop {
        byte_buf.clear();
        let bytes_read = reader.read_until(b'\n', &mut byte_buf)?;
        if bytes_read == 0 {
            break;
        }

        let raw_line = String::from_utf8_lossy(&byte_buf);
        let mut line = raw_line.as_ref();
        if is_first_line {
            is_first_line = false;
            line = line.strip_prefix('\u{FEFF}').unwrap_or(line);
        }

        let trimmed_end = line.trim_end_matches(['\r', '\n']);
        for subline in trimmed_end.split('\r') {
            let trimmed = subline.trim();
            if trimmed.is_empty() {
                if !current_block.is_empty() {
                    write_cue_block(&mut writer, &current_block, &mut has_written_header)?;
                    current_block.clear();
                }
            } else {
                current_block.push(subline.to_string());
            }
        }
    }

    if !current_block.is_empty() {
        write_cue_block(&mut writer, &current_block, &mut has_written_header)?;
        current_block.clear();
    }

    if !has_written_header {
        writer.write_all(b"WEBVTT\n")?;
    }

    writer.flush()?;
    Ok(())
}

/// Converts SubRip (SRT) format subtitle content into valid WebVTT format.
///
/// Backwards-compatible wrapper around [`srt_to_webvtt_stream`].
pub fn srt_to_webvtt(srt: &str) -> String {
    let mut out = Vec::new();
    let reader = std::io::Cursor::new(srt.as_bytes());
    let _ = srt_to_webvtt_stream(reader, &mut out);
    String::from_utf8(out).unwrap_or_else(|_| "WEBVTT\n".to_string())
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
        assert_eq!(
            clean_cue_text("<i>Hello</i> <font color=\"red\">World</font>"),
            "<i>Hello</i> World"
        );
        assert_eq!(clean_cue_text("{\\an8}{\\b1}Bold and top"), "Bold and top");
    }

    #[test]
    fn test_skip_malformed_blocks() {
        let srt = "Invalid block without timing\nJust some random text\n\n1\n00:00:01,000 --> 00:00:02,000\nValid cue\n";
        let vtt = srt_to_webvtt(srt);
        assert_eq!(
            vtt,
            "WEBVTT\n\n1\n00:00:01.000 --> 00:00:02.000\nValid cue\n"
        );
    }

    #[test]
    fn test_srt_to_webvtt_stream_direct() {
        let srt = "\u{FEFF}1\r\n00:00:01,000 --> 00:00:02,000\r\nDirect streaming line\r\n";
        let mut out = Vec::new();
        srt_to_webvtt_stream(std::io::BufReader::new(srt.as_bytes()), &mut out).unwrap();
        let res = String::from_utf8(out).unwrap();
        assert_eq!(
            res,
            "WEBVTT\n\n1\n00:00:01.000 --> 00:00:02.000\nDirect streaming line\n"
        );
    }
}
