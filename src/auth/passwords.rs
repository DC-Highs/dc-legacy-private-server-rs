//! Argon2id password hashing (the parameters match the Python version, so old hashes verify).
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};

pub const MIN_PASSWORD_LENGTH: usize = 8;
pub const MAX_PASSWORD_LENGTH: usize = 128;

const MEMORY_KIB: u32 = 65_536;
const ITERATIONS: u32 = 3;
const PARALLELISM: u32 = 4;

fn hasher() -> Argon2<'static> {
    let params = Params::new(MEMORY_KIB, ITERATIONS, PARALLELISM, None).expect("valid argon2 parameters");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

pub fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    hasher()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .unwrap_or_default()
}

pub fn verify_password(password_hash: &str, password: &str) -> bool {
    match PasswordHash::new(password_hash) {
        Ok(parsed) => Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok(),
        Err(_) => false,
    }
}

pub fn needs_rehash(password_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(password_hash) else { return true };
    match Params::try_from(&parsed) {
        Ok(params) => {
            params.m_cost() != MEMORY_KIB || params.t_cost() != ITERATIONS || params.p_cost() != PARALLELISM
                || parsed.algorithm.as_str() != "argon2id"
        }
        Err(_) => true,
    }
}

pub async fn hash_password_async(password: String) -> String {
    tokio::task::spawn_blocking(move || hash_password(&password)).await.unwrap_or_default()
}

pub async fn verify_password_async(password_hash: String, password: String) -> bool {
    tokio::task::spawn_blocking(move || verify_password(&password_hash, &password)).await.unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_verify_and_are_argon2id() {
        let hash = hash_password("correct horse battery");
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password(&hash, "correct horse battery"));
        assert!(!verify_password(&hash, "wrong"));
        assert!(!needs_rehash(&hash));
        assert!(!verify_password("not a hash", "x"));
    }
}
