use crate::error::Result;
use kadr_core::models::TechnicalInfo;
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub fn inspect_container<P: AsRef<Path>>(path: P) -> Result<TechnicalInfo> {
    let mut file = match File::open(path.as_ref()) {
        Ok(f) => f,
        Err(_) => return Ok(TechnicalInfo::default()),
    };

    let mut header = [0u8; 16];
    let bytes_read = file.read(&mut header).unwrap_or(0);
    let mut info = TechnicalInfo::default();

    if bytes_read >= 4 && header[0..4] == [0x1A, 0x45, 0xDF, 0xA3] {
        info.container = Some("mkv".to_string());
    } else if bytes_read >= 8 && header[4..8] == *b"ftyp" {
        info.container = Some("mp4".to_string());
    } else if let Some(ext) = path.as_ref().extension().and_then(|s| s.to_str()) {
        info.container = Some(ext.to_lowercase());
    }

    Ok(info)
}
