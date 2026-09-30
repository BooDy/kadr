#[derive(Debug, PartialEq, Eq)]
pub enum ByteRange {
    Exact { start: u64, end: u64 },
    OpenEnded { start: u64 },
    Suffix { suffix: u64 },
}

pub fn parse_range_header(header: &str, total_size: u64) -> Option<(u64, u64)> {
    let header = header.trim();
    if !header.starts_with("bytes=") {
        return None;
    }
    let spec = &header["bytes=".len()..];
    // We only support single ranges per RFC 7233 standard direct play
    let range_part = spec.split(',').next()?.trim();

    if let Some(suffix_str) = range_part.strip_prefix('-') {
        let suffix: u64 = suffix_str.parse().ok()?;
        if suffix == 0 || total_size == 0 {
            return None;
        }
        let start = total_size.saturating_sub(suffix);
        let end = total_size - 1;
        Some((start, end))
    } else {
        let mut parts = range_part.split('-');
        let start_str = parts.next()?.trim();
        let end_str = parts.next()?.trim();

        let start: u64 = start_str.parse().ok()?;
        if start >= total_size {
            return None;
        }

        if end_str.is_empty() {
            Some((start, total_size - 1))
        } else {
            let end: u64 = end_str.parse().ok()?;
            if end < start {
                return None;
            }
            let clamped_end = end.min(total_size - 1);
            Some((start, clamped_end))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_range_exact() {
        assert_eq!(parse_range_header("bytes=100-199", 1000), Some((100, 199)));
        assert_eq!(parse_range_header("bytes=0-0", 1000), Some((0, 0)));
        assert_eq!(parse_range_header("bytes=0-999", 1000), Some((0, 999)));
    }

    #[test]
    fn test_parse_range_open_ended() {
        assert_eq!(parse_range_header("bytes=500-", 1000), Some((500, 999)));
        assert_eq!(parse_range_header("bytes=0-", 1000), Some((0, 999)));
    }

    #[test]
    fn test_parse_range_suffix() {
        assert_eq!(parse_range_header("bytes=-100", 1000), Some((900, 999)));
        // Suffix larger than file
        assert_eq!(parse_range_header("bytes=-1500", 1000), Some((0, 999)));
        // Suffix 0 is invalid
        assert_eq!(parse_range_header("bytes=-0", 1000), None);
    }

    #[test]
    fn test_parse_range_clamped_end() {
        assert_eq!(parse_range_header("bytes=500-2000", 1000), Some((500, 999)));
    }

    #[test]
    fn test_parse_range_unsatisfiable() {
        // start >= total_size
        assert_eq!(parse_range_header("bytes=1000-1050", 1000), None);
        assert_eq!(parse_range_header("bytes=1000-", 1000), None);
        // end < start
        assert_eq!(parse_range_header("bytes=500-400", 1000), None);
        // empty file
        assert_eq!(parse_range_header("bytes=0-100", 0), None);
        assert_eq!(parse_range_header("bytes=-50", 0), None);
    }

    #[test]
    fn test_parse_range_invalid_syntax() {
        assert_eq!(parse_range_header("invalid", 1000), None);
        assert_eq!(parse_range_header("bytes=", 1000), None);
        assert_eq!(parse_range_header("bytes=abc-def", 1000), None);
        assert_eq!(parse_range_header("bytes=100", 1000), None);
        assert_eq!(parse_range_header("chars=0-100", 1000), None);
    }

    #[test]
    fn test_parse_range_first_part_of_multiple() {
        assert_eq!(parse_range_header("bytes=0-50, 100-150", 1000), Some((0, 50)));
    }
}
