use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerIdentity {
    pub id: String,
}

pub fn load_or_create_server_id(data_dir: &Path) -> Result<String, std::io::Error> {
    let id_file = data_dir.join("server_id");
    if id_file.is_file() {
        if let Ok(content) = std::fs::read_to_string(&id_file) {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
    }
    std::fs::create_dir_all(data_dir)?;
    let new_id = uuid::Uuid::new_v4().to_string();
    std::fs::write(&id_file, &new_id)?;
    Ok(new_id)
}
