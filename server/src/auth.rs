use crate::util::*;
use crate::App;
use axum::{async_trait, extract::FromRequestParts, http::request::Parts, http::HeaderMap};
use rusqlite::params;
use std::sync::Arc;

pub struct Auth {
    pub role: String,
    pub nama: String,
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
        conn.query_row(
            "SELECT role, nama FROM sessions WHERE token = ?1 AND expires_at > ?2",
            params![tok, now()],
            |r| {
                Ok(Auth {
                    role: r.get(0)?,
                    nama: r.get(1)?,
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
