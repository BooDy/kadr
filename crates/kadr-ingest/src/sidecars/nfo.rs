use std::fs;
use std::path::Path;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use crate::error::Result;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NfoData {
    pub title: Option<String>,
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub director: Option<String>,
    pub studio: Option<String>,
    pub actors: Vec<String>,
    pub tags: Vec<String>,
}

pub fn parse_nfo<P: AsRef<Path>>(path: P) -> Result<Option<NfoData>> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };
    let content = String::from_utf8_lossy(&bytes);

    let mut reader = Reader::from_str(&content);
    reader.config_mut().trim_text(true);

    let mut nfo = NfoData::default();
    let mut current_tag = String::new();
    let mut in_actor = false;

    let parse_year = |s: &str| -> Option<i32> {
        if let Ok(y) = s.parse::<i32>() {
            return Some(y);
        }
        if s.len() >= 4 {
            if let Ok(y) = s[..4].parse::<i32>() {
                return Some(y);
            }
        }
        None
    };

    loop {
        match reader.read_event()? {
            Event::Start(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_lowercase();
                if name == "actor" {
                    in_actor = true;
                }
                current_tag = name;
            }
            Event::End(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_lowercase();
                if name == "actor" {
                    in_actor = false;
                }
                current_tag.clear();
            }
            Event::Text(e) => {
                let text = e.unescape()?.trim().to_string();
                if text.is_empty() {
                    continue;
                }
                match (current_tag.as_str(), in_actor) {
                    ("title", false) => nfo.title = Some(text),
                    ("originaltitle", false) | ("original_title", false) => {
                        nfo.original_title = Some(text);
                    }
                    ("year", false) => {
                        nfo.year = parse_year(&text).or(nfo.year);
                    }
                    ("premiered", false) | ("releasedate", false) => {
                        if nfo.year.is_none() {
                            nfo.year = parse_year(&text);
                        }
                    }
                    ("plot", false) | ("overview", false) => nfo.overview = Some(text),
                    ("director", false) => nfo.director = Some(text),
                    ("studio", false) => nfo.studio = Some(text),
                    ("name", true) => nfo.actors.push(text),
                    ("genre", false) | ("tag", false) => nfo.tags.push(text),
                    _ => {}
                }
            }
            Event::CData(e) => {
                let text = String::from_utf8_lossy(&e).trim().to_string();
                if text.is_empty() {
                    continue;
                }
                match (current_tag.as_str(), in_actor) {
                    ("title", false) => nfo.title = Some(text),
                    ("originaltitle", false) | ("original_title", false) => {
                        nfo.original_title = Some(text);
                    }
                    ("year", false) => {
                        nfo.year = parse_year(&text).or(nfo.year);
                    }
                    ("premiered", false) | ("releasedate", false) => {
                        if nfo.year.is_none() {
                            nfo.year = parse_year(&text);
                        }
                    }
                    ("plot", false) | ("overview", false) => nfo.overview = Some(text),
                    ("director", false) => nfo.director = Some(text),
                    ("studio", false) => nfo.studio = Some(text),
                    ("name", true) => nfo.actors.push(text),
                    ("genre", false) | ("tag", false) => nfo.tags.push(text),
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    Ok(Some(nfo))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_cdata_and_case_insensitive_tags() {
        let xml = r#"<MOVIE>
            <TITLE><![CDATA[The Matrix & Reloaded]]></TITLE>
            <ORIGINAL_TITLE><![CDATA[The Matrix]]></ORIGINAL_TITLE>
            <PREMIERED>1999-03-31</PREMIERED>
            <PLOT><![CDATA[A computer hacker learns from mysterious rebels about the true nature of his reality.]]></PLOT>
            <DIRECTOR>Lana Wachowski</DIRECTOR>
            <STUDIO>Warner Bros.</STUDIO>
            <ACTOR>
                <NAME>Keanu Reeves</NAME>
                <ROLE>Neo</ROLE>
            </ACTOR>
            <GENRE>Action</GENRE>
            <TAG>Sci-Fi</TAG>
        </MOVIE>"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(xml.as_bytes()).unwrap();

        let nfo = parse_nfo(file.path()).unwrap().expect("should parse");
        assert_eq!(nfo.title.as_deref(), Some("The Matrix & Reloaded"));
        assert_eq!(nfo.original_title.as_deref(), Some("The Matrix"));
        assert_eq!(nfo.year, Some(1999));
        assert_eq!(
            nfo.overview.as_deref(),
            Some("A computer hacker learns from mysterious rebels about the true nature of his reality.")
        );
        assert_eq!(nfo.director.as_deref(), Some("Lana Wachowski"));
        assert_eq!(nfo.studio.as_deref(), Some("Warner Bros."));
        assert_eq!(nfo.actors, vec!["Keanu Reeves"]);
        assert_eq!(nfo.tags, vec!["Action", "Sci-Fi"]);
    }

    #[test]
    fn test_malformed_xml_returns_error() {
        let malformed = "<movie><title>Unclosed</movie>";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(malformed.as_bytes()).unwrap();

        let res = parse_nfo(file.path());
        assert!(res.is_err());
    }

    #[test]
    fn test_non_utf8_lossy_graceful_handling() {
        // Create file with Latin-1 bytes e.g. 0xE9 for 'é'
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"<movie><title>Am");
        bytes.push(0xE9); // 'é' in Latin-1
        bytes.extend_from_slice(b"lie</title></movie>");

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&bytes).unwrap();

        let nfo = parse_nfo(file.path()).unwrap().expect("should parse lossy");
        assert!(nfo.title.unwrap().starts_with("Am"));
    }
}
