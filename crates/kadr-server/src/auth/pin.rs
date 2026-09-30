use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PinError {
    #[error("PIN must consist of exactly 4 numeric digits")]
    InvalidFormat,
    #[error("Argon2 hashing error: {0}")]
    HashError(String),
}

pub fn validate_pin(pin: &str) -> Result<(), PinError> {
    if pin.len() == 4 && pin.chars().all(|c| c.is_ascii_digit()) {
        Ok(())
    } else {
        Err(PinError::InvalidFormat)
    }
}

pub fn hash_pin(pin: &str) -> Result<String, PinError> {
    validate_pin(pin)?;
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(pin.as_bytes(), &salt)
        .map_err(|e| PinError::HashError(e.to_string()))?
        .to_string();
    Ok(hash)
}

pub fn verify_pin(pin: &str, pin_hash: &str) -> Result<bool, PinError> {
    let parsed_hash = match PasswordHash::new(pin_hash) {
        Ok(h) => h,
        Err(_) => return Ok(false),
    };
    let argon2 = Argon2::default();
    Ok(argon2.verify_password(pin.as_bytes(), &parsed_hash).is_ok())
}
