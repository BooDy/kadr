#[derive(Debug, PartialEq, Eq)]
pub enum RangeResult {
    Satisfiable { start: u64, end: u64 },
    Unsatisfiable,
    Ignore,
}

pub fn parse_range_header(header: &str, total_size: u64) -> RangeResult {
    let header = header.trim();
    if !header.starts_with("bytes=") {
        return RangeResult::Ignore;
    }
    let spec = &header["bytes=".len()..];
    // We only support single ranges per RFC 7233 standard direct play
    let range_part = match spec.split(',').next() {
        Some(p) => p.trim(),
        None => return RangeResult::Ignore,
    };
    if range_part.is_empty() {
        return RangeResult::Ignore;
    }

    if let Some(suffix_str) = range_part.strip_prefix('-') {
        let suffix_str = suffix_str.trim();
        if suffix_str.is_empty() {
            return RangeResult::Ignore;
        }
        let suffix: u64 = match suffix_str.parse() {
            Ok(s) => s,
            Err(_) => return RangeResult::Ignore,
        };
        if suffix == 0 || total_size == 0 {
            return RangeResult::Unsatisfiable;
        }
        let start = total_size.saturating_sub(suffix);
        let end = total_size - 1;
        RangeResult::Satisfiable { start, end }
    } else {
        let mut parts = range_part.split('-');
        let start_str = match parts.next() {
            Some(s) => s.trim(),
            None => return RangeResult::Ignore,
        };
        let end_str = match parts.next() {
            Some(e) => e.trim(),
            None => return RangeResult::Ignore,
        };
        if parts.next().is_some() {
            return RangeResult::Ignore;
        }

        let start: u64 = match start_str.parse() {
            Ok(s) => s,
            Err(_) => return RangeResult::Ignore,
        };
        if total_size == 0 || start >= total_size {
            return RangeResult::Unsatisfiable;
        }

        if end_str.is_empty() {
            RangeResult::Satisfiable {
                start,
                end: total_size - 1,
            }
        } else {
            let end: u64 = match end_str.parse() {
                Ok(e) => e,
                Err(_) => return RangeResult::Ignore,
            };
            if end < start {
                return RangeResult::Unsatisfiable;
            }
            let clamped_end = end.min(total_size - 1);
            RangeResult::Satisfiable {
                start,
                end: clamped_end,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_range_exact() {
        assert_eq!(
            parse_range_header("bytes=100-199", 1000),
            RangeResult::Satisfiable {
                start: 100,
                end: 199
            }
        );
        assert_eq!(
            parse_range_header("bytes=0-0", 1000),
            RangeResult::Satisfiable { start: 0, end: 0 }
        );
        assert_eq!(
            parse_range_header("bytes=0-999", 1000),
            RangeResult::Satisfiable { start: 0, end: 999 }
        );
    }

    #[test]
    fn test_parse_range_open_ended() {
        assert_eq!(
            parse_range_header("bytes=500-", 1000),
            RangeResult::Satisfiable {
                start: 500,
                end: 999
            }
        );
        assert_eq!(
            parse_range_header("bytes=0-", 1000),
            RangeResult::Satisfiable { start: 0, end: 999 }
        );
    }

    #[test]
    fn test_parse_range_suffix() {
        assert_eq!(
            parse_range_header("bytes=-100", 1000),
            RangeResult::Satisfiable {
                start: 900,
                end: 999
            }
        );
        // Suffix larger than file
        assert_eq!(
            parse_range_header("bytes=-1500", 1000),
            RangeResult::Satisfiable { start: 0, end: 999 }
        );
        // Suffix 0 is invalid
        assert_eq!(
            parse_range_header("bytes=-0", 1000),
            RangeResult::Unsatisfiable
        );
    }

    #[test]
    fn test_parse_range_clamped_end() {
        assert_eq!(
            parse_range_header("bytes=500-2000", 1000),
            RangeResult::Satisfiable {
                start: 500,
                end: 999
            }
        );
    }

    #[test]
    fn test_parse_range_unsatisfiable() {
        // start >= total_size
        assert_eq!(
            parse_range_header("bytes=1000-1050", 1000),
            RangeResult::Unsatisfiable
        );
        assert_eq!(
            parse_range_header("bytes=1000-", 1000),
            RangeResult::Unsatisfiable
        );
        assert_eq!(
            parse_range_header("bytes=9999-", 1000),
            RangeResult::Unsatisfiable
        );
        // end < start
        assert_eq!(
            parse_range_header("bytes=500-400", 1000),
            RangeResult::Unsatisfiable
        );
        // empty file
        assert_eq!(
            parse_range_header("bytes=0-100", 0),
            RangeResult::Unsatisfiable
        );
        assert_eq!(
            parse_range_header("bytes=-50", 0),
            RangeResult::Unsatisfiable
        );
    }

    #[test]
    fn test_parse_range_invalid_syntax() {
        assert_eq!(parse_range_header("invalid", 1000), RangeResult::Ignore);
        assert_eq!(parse_range_header("bytes=", 1000), RangeResult::Ignore);
        assert_eq!(
            parse_range_header("bytes=abc-def", 1000),
            RangeResult::Ignore
        );
        assert_eq!(parse_range_header("bytes=100", 1000), RangeResult::Ignore);
        assert_eq!(
            parse_range_header("chars=0-100", 1000),
            RangeResult::Ignore
        );
        assert_eq!(parse_range_header("bytes=-", 1000), RangeResult::Ignore);
        assert_eq!(
            parse_range_header("bytes=1-2-3", 1000),
            RangeResult::Ignore
        );
    }

    #[test]
    fn test_parse_range_first_part_of_multiple() {
        assert_eq!(
            parse_range_header("bytes=0-50, 100-150", 1000),
            RangeResult::Satisfiable { start: 0, end: 50 }
        );
    }
}
