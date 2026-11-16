use crate::error::Result;
use kadr_core::models::TechnicalInfo;
use std::fs::File;
use std::io::Read;
use std::path::Path;

fn detect_magic_container(header: &[u8]) -> Option<&'static str> {
    if header.len() >= 4 && header[0..4] == [0x1A, 0x45, 0xDF, 0xA3] {
        let is_webm = header
            .windows(4)
            .any(|window| window.eq_ignore_ascii_case(b"webm"));
        return Some(if is_webm { "webm" } else { "mkv" });
    }

    if header.len() >= 8 && &header[4..8] == b"ftyp" {
        if header.len() >= 12 {
            let brand = &header[8..12];
            if brand == b"qt  " {
                return Some("mov");
            }
            if brand.eq_ignore_ascii_case(b"m4v ")
                || brand.eq_ignore_ascii_case(b"m4vh")
                || brand.eq_ignore_ascii_case(b"m4vp")
            {
                return Some("m4v");
            }
            let prefix3 = &brand[0..3];
            if prefix3.eq_ignore_ascii_case(b"3gp")
                || prefix3.eq_ignore_ascii_case(b"3ge")
                || prefix3.eq_ignore_ascii_case(b"3gg")
            {
                return Some("3gp");
            }
            if prefix3.eq_ignore_ascii_case(b"3g2") {
                return Some("3g2");
            }
        }
        return Some("mp4");
    }

    if header.len() >= 12 && &header[0..4] == b"RIFF" && &header[8..12] == b"AVI " {
        return Some("avi");
    }

    if header.len() > 188 && header[0] == 0x47 && header[188] == 0x47 {
        return Some("ts");
    }

    if header.len() > 196 && header[4] == 0x47 && header[196] == 0x47 {
        return Some("m2ts");
    }

    None
}

pub fn inspect_container<P: AsRef<Path>>(path: P) -> Result<TechnicalInfo> {
    let mut file = match File::open(path.as_ref()) {
        Ok(f) => f,
        Err(_) => return Ok(TechnicalInfo::default()),
    };

    let mut header = [0u8; 256];
    let mut bytes_read = 0;
    while bytes_read < header.len() {
        match file.read(&mut header[bytes_read..]) {
            Ok(0) => break,
            Ok(n) => bytes_read += n,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }

    let mut info = TechnicalInfo::default();
    if let Some(detected) = detect_magic_container(&header[..bytes_read]) {
        info.container = Some(detected.to_string());
    } else if let Some(ext) = path.as_ref().extension().and_then(|s| s.to_str()) {
        info.container = Some(ext.to_lowercase());
    }

    Ok(info)
}
