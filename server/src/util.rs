use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use rusqlite::Connection;
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct AppError(pub StatusCode, pub String);
pub type R<T> = Result<T, AppError>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        eprintln!("[db] {e}");
        AppError(StatusCode::INTERNAL_SERVER_ERROR, "kesalahan database".into())
    }
}

pub fn bad(msg: &str) -> AppError {
    AppError(StatusCode::BAD_REQUEST, msg.to_string())
}
pub fn not_found(msg: &str) -> AppError {
    AppError(StatusCode::NOT_FOUND, msg.to_string())
}
pub fn conflict(msg: &str) -> AppError {
    AppError(StatusCode::CONFLICT, msg.to_string())
}
pub fn unauthorized() -> AppError {
    AppError(StatusCode::UNAUTHORIZED, "belum login".to_string())
}
pub fn forbidden() -> AppError {
    AppError(StatusCode::FORBIDDEN, "tidak punya akses".to_string())
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 143 -> "DPK-0143"
pub fn kode(id: i64) -> String {
    format!("DPK-{:04}", id)
}

/// 36000 -> "Rp36.000"
pub fn rupiah(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    format!("{}Rp{}", if n < 0 { "-" } else { "" }, out)
}

pub fn setting_s(conn: &Connection, key: &str, def: &str) -> String {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
        r.get::<_, String>(0)
    })
    .unwrap_or_else(|_| def.to_string())
}

pub fn setting_i(conn: &Connection, key: &str, def: i64) -> i64 {
    setting_s(conn, key, "").parse::<i64>().unwrap_or(def)
}

/// Awal hari (epoch detik, UTC) menurut zona waktu restoran.
pub fn day_start(t: i64, tz_off_min: i64) -> i64 {
    let off = tz_off_min * 60;
    (t + off).div_euclid(86400) * 86400 - off
}

pub fn rand_token(n: usize) -> String {
    use rand::Rng;
    const A: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut rng = rand::thread_rng();
    (0..n)
        .map(|_| A[rng.gen_range(0..A.len())] as char)
        .collect()
}
