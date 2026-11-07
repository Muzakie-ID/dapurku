use crate::App;
use axum::{
    http::HeaderMap,
    response::{
        sse::{Event, KeepAlive},
        Sse,
    },
};
use futures_core::Stream;
use rusqlite::{params, Connection};
use std::{collections::VecDeque, convert::Infallible, sync::Arc, time::Duration};

/// Titik awal stream: `Last-Event-ID` kalau browser menyambung ulang,
/// kalau tidak, id notifikasi terakhir (tidak mengulang riwayat lama).
pub fn start_id(conn: &Connection, headers: &HeaderMap) -> i64 {
    if let Some(v) = headers.get("last-event-id").and_then(|v| v.to_str().ok()) {
        if let Ok(n) = v.parse::<i64>() {
            return n;
        }
    }
    conn.query_row("SELECT COALESCE(MAX(id), 0) FROM notifications", [], |r| r.get(0))
        .unwrap_or(0)
}

struct St {
    app: Arc<App>,
    order_id: Option<i64>,
    audiences: Vec<String>,
    last: i64,
    buf: VecDeque<(i64, String)>,
}

fn fetch(st: &St) -> Vec<(i64, String)> {
    let a1 = st.audiences.get(0).cloned().unwrap_or_default();
    let a2 = st.audiences.get(1).cloned().unwrap_or_else(|| a1.clone());
    let conn = st.app.db.lock().unwrap();
    let mut out = Vec::new();
    let stmt = conn.prepare(
        "SELECT n.id, n.tipe, n.judul, n.isi, n.created_at, n.audience, n.order_id
           FROM notifications n
          WHERE n.id > ?1 AND (n.audience = ?2 OR n.audience = ?3)
            AND (?4 IS NULL OR n.order_id = ?4)
          ORDER BY n.id LIMIT 50",
    );
    if let Ok(mut stmt) = stmt {
        let rows = stmt.query_map(params![st.last, a1, a2, st.order_id], |r| {
            let id: i64 = r.get(0)?;
            let oid: Option<i64> = r.get(6)?;
            let data = serde_json::json!({
                "id": id, "tipe": r.get::<_, String>(1)?, "judul": r.get::<_, String>(2)?,
                "isi": r.get::<_, String>(3)?, "at": r.get::<_, i64>(4)?,
                "audience": r.get::<_, String>(5)?, "order_id": oid,
            })
            .to_string();
            Ok((id, data))
        });
        if let Ok(rows) = rows {
            for row in rows.flatten() {
                out.push(row);
            }
        }
    }
    out
}

/// Stream SSE sederhana: polling tabel `notifications` tiap detik.
/// Cukup untuk restoran kecil dan otomatis menangkap notifikasi dari worker Python.
pub fn stream(
    app: Arc<App>,
    order_id: Option<i64>,
    audiences: Vec<String>,
    last: i64,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let st = St { app, order_id, audiences, last, buf: VecDeque::new() };
    let s = futures_util::stream::unfold(st, |mut st| async move {
        loop {
            if let Some((id, data)) = st.buf.pop_front() {
                let ev = Event::default().id(id.to_string()).event("notif").data(data);
                return Some((Ok::<Event, Infallible>(ev), st));
            }
            let rows = fetch(&st);
            for r in rows {
                st.last = r.0;
                st.buf.push_back(r);
            }
            if st.buf.is_empty() {
                tokio::time::sleep(Duration::from_millis(1000)).await;
            }
        }
    });
    Sse::new(s).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
