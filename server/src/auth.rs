use crate::util::*;
use crate::App;
use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::{async_trait, extract::FromRequestParts, http::request::Parts, http::HeaderMap};
use rusqlite::params;
use std::sync::Arc;

pub struct Auth {
    pub role: String,
    pub nama: String,
    pub username: String,
}

/// Hash password dengan Argon2id. Hasilnya format PHC, salt sudah ikut di dalamnya.
pub fn hash_password(pw: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .expect("gagal menghitung hash password")
        .to_string()
}

/// Cocokkan password dengan hash tersimpan. Hash rusak = selalu gagal, bukan panik.
pub fn verify_password(hash: &str, pw: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(h) => Argon2::default().verify_password(pw.as_bytes(), &h).is_ok(),
        Err(_) => false,
    }
}

pub fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get("cookie")?.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix(&format!("{name}=")) {
            return Some(v.to_string());
        }
    }
    None
}

#[async_trait]
impl FromRequestParts<Arc<App>> for Auth {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<App>) -> Result<Self, Self::Rejection> {
        let tok = cookie(&parts.headers, "dk_session").ok_or_else(unauthorized)?;
        let conn = state.db.lock().unwrap();
        // Akun yang dinonaktifkan langsung kehilangan sesinya.
        conn.query_row(
            "SELECT s.role, s.nama, COALESCE(u.username, '')
               FROM sessions s LEFT JOIN users u ON u.id = s.user_id
              WHERE s.token = ?1 AND s.expires_at > ?2 AND (s.user_id IS NULL OR u.aktif = 1)",
            params![tok, now()],
            |r| {
                Ok(Auth {
                    role: r.get(0)?,
                    nama: r.get(1)?,
                    username: r.get(2)?,
                })
            },
        )
        .map_err(|_| unauthorized())
    }
}

impl Auth {
    /// Admin boleh semuanya. Role lain harus ada di daftar.
    pub fn need(&self, roles: &[&str]) -> R<()> {
        if self.role == "admin" || roles.contains(&self.role.as_str()) {
            Ok(())
        } else {
            Err(forbidden())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_cocok_dengan_hashnya() {
        let h = hash_password("rahasia123");
        assert!(verify_password(&h, "rahasia123"));
        assert!(!verify_password(&h, "rahasia124"));
        assert!(!verify_password(&h, ""));
    }

    #[test]
    fn hash_tidak_menyimpan_password_apa_adanya() {
        let h = hash_password("rahasia123");
        assert!(!h.contains("rahasia123"));
        assert!(h.starts_with("$argon2"));
    }

    #[test]
    fn dua_hash_dari_password_sama_tetap_berbeda() {
        // salt acak: dua pengguna dengan password sama tidak terlihat sama di database
        assert_ne!(hash_password("rahasia123"), hash_password("rahasia123"));
    }

    #[test]
    fn hash_rusak_ditolak_bukan_panik() {
        assert!(!verify_password("bukan-hash", "apa pun"));
        assert!(!verify_password("", "apa pun"));
    }
}
