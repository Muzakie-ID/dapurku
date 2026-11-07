use crate::util::now;
use rusqlite::{params, Connection};

/// Tulis satu notifikasi. `dedupe` mencegah notifikasi yang sama terkirim dua kali.
/// Notifikasi untuk pembeli ditandai `pending` supaya worker Python bisa meneruskannya
/// ke kanal luar (WhatsApp, webhook, dll). Untuk staf cukup `skip`.
pub fn push(
    conn: &Connection,
    order_id: Option<i64>,
    audience: &str,
    tipe: &str,
    judul: &str,
    isi: &str,
    dedupe: Option<&str>,
) -> rusqlite::Result<()> {
    let ext = if audience == "pembeli" { "pending" } else { "skip" };
    conn.execute(
        "INSERT OR IGNORE INTO notifications
           (order_id, audience, tipe, judul, isi, dedupe_key, ext_status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![order_id, audience, tipe, judul, isi, dedupe, ext, now()],
    )?;
    Ok(())
}
