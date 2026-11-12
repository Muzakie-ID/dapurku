use crate::notify::push;
use crate::util::*;
use axum::http::StatusCode;
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
pub struct NewItem {
    pub menu_id: i64,
    pub qty: i64,
    pub level: Option<i64>,
    #[serde(default)]
    pub extras: Vec<i64>,
    pub catatan: Option<String>,
}

#[derive(Deserialize)]
pub struct NewOrder {
    pub meja: String,
    pub metode: String,
    pub items: Vec<NewItem>,
}

struct Line {
    menu_id: i64,
    nama: String,
    unit: i64,
    qty: i64,
    level: Option<i64>,
    extras: String,
    catatan: String,
}

/// Buat pesanan baru. Harga dihitung ulang di server, bukan percaya angka dari klien.
pub fn create_order(conn: &Connection, o: &NewOrder) -> R<(i64, String)> {
    if o.metode != "qris" && o.metode != "tunai" {
        return Err(bad("metode bayar tidak dikenal"));
    }
    if o.items.is_empty() || o.items.len() > 40 {
        return Err(bad("keranjang kosong atau terlalu banyak"));
    }
    let tx = conn.unchecked_transaction()?;

    let ada: i64 = tx.query_row(
        "SELECT COUNT(*) FROM meja WHERE no = ?1 AND aktif = 1",
        params![o.meja],
        |r| r.get(0),
    )?;
    if ada == 0 {
        return Err(bad("nomor meja tidak valid"));
    }

    let mut total = 0i64;
    let mut lines: Vec<Line> = Vec::new();
    for it in &o.items {
        if it.qty < 1 || it.qty > 50 {
            return Err(bad("jumlah tidak valid"));
        }
        let (nama, harga, pedas, tersedia): (String, i64, i64, i64) = tx
            .query_row(
                "SELECT nama, harga, pedas, tersedia FROM menu_items WHERE id = ?1 AND aktif = 1",
                params![it.menu_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .map_err(|_| bad("menu tidak ditemukan"))?;
        if tersedia == 0 {
            return Err(bad(&format!("{} sedang habis", nama)));
        }
        let mut unit = harga;
        let mut names: Vec<String> = Vec::new();
        for eid in &it.extras {
            let (en, eh): (String, i64) = tx
                .query_row(
                    "SELECT nama, harga FROM menu_extras WHERE id = ?1 AND menu_id = ?2",
                    params![eid, it.menu_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(|_| bad("pilihan tambahan tidak valid"))?;
            unit += eh;
            names.push(en);
        }
        let level = if pedas == 1 {
            Some(it.level.unwrap_or(0).clamp(0, 5))
        } else {
            None
        };
        let catatan: String = it
            .catatan
            .clone()
            .unwrap_or_default()
            .trim()
            .chars()
            .take(140)
            .collect();
        total += unit * it.qty;
        lines.push(Line {
            menu_id: it.menu_id,
            nama,
            unit,
            qty: it.qty,
            level,
            extras: names.join(", "),
            catatan,
        });
    }

    let n = now();
    let menit = if o.metode == "qris" {
        setting_i(&tx, "bayar_menit", 15)
    } else {
        setting_i(&tx, "tunai_menit", 30)
    };
    let token = rand_token(16);
    tx.execute(
        "INSERT INTO orders(token, meja_no, metode, status, total, created_at, expires_at)
         VALUES (?1, ?2, ?3, 'menunggu_bayar', ?4, ?5, ?6)",
        params![token, o.meja, o.metode, total, n, n + menit * 60],
    )?;
    let id = tx.last_insert_rowid();
    for l in &lines {
        tx.execute(
            "INSERT INTO order_items(order_id, menu_id, nama, harga, qty, level_pedas, extras, catatan)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, l.menu_id, l.nama, l.unit, l.qty, l.level, l.extras, l.catatan],
        )?;
    }

    // Notifikasi: pesanan masuk
    let k = kode(id);
    if o.metode == "qris" {
        push(
            &tx, Some(id), "pembeli", "pesanan_masuk", "Pesanan diterima",
            &format!(
                "Pesanan #{k} untuk Meja {} sudah kami catat. Selesaikan pembayaran QRIS dalam {menit} menit.",
                o.meja
            ),
            Some(&format!("{id}:pesanan_masuk")),
        )?;
    } else {
        push(
            &tx, Some(id), "pembeli", "pesanan_masuk", "Pesanan diterima",
            &format!(
                "Pesanan #{k} untuk Meja {} sudah kami catat. Silakan bayar tunai di kasir dalam {menit} menit.",
                o.meja
            ),
            Some(&format!("{id}:pesanan_masuk")),
        )?;
        push(
            &tx, Some(id), "kasir", "tunai_menunggu", &format!("Menunggu bayar tunai · Meja {}", o.meja),
            &format!("#{k} · {}", rupiah(total)),
            Some(&format!("{id}:tunai_menunggu")),
        )?;
    }
    tx.commit()?;
    Ok((id, token))
}

pub fn items_text(conn: &Connection, order_id: i64) -> rusqlite::Result<String> {
    let mut stmt = conn.prepare("SELECT qty, nama FROM order_items WHERE order_id = ?1 ORDER BY id")?;
    let rows = stmt.query_map(params![order_id], |r| {
        Ok(format!("{}× {}", r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    })?;
    let v: Vec<String> = rows.collect::<Result<Vec<_>, _>>()?;
    Ok(v.join(", "))
}

/// Daftar item dalam bentuk JSON. `note` = ringkasan opsi untuk layar dapur.
pub fn items_json(conn: &Connection, order_id: i64) -> rusqlite::Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT qty, nama, harga, level_pedas, extras, catatan FROM order_items WHERE order_id = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map(params![order_id], |r| {
        let qty: i64 = r.get(0)?;
        let nama: String = r.get(1)?;
        let harga: i64 = r.get(2)?;
        let level: Option<i64> = r.get(3)?;
        let extras: String = r.get(4)?;
        let catatan: String = r.get(5)?;
        let mut parts: Vec<String> = Vec::new();
        if let Some(l) = level {
            let mut s = format!("Level {l}");
            if !extras.is_empty() {
                s.push_str(&format!(" + {}", extras.to_lowercase()));
            }
            parts.push(s);
        } else if !extras.is_empty() {
            parts.push(format!("+ {}", extras.to_lowercase()));
        }
        if !catatan.is_empty() {
            parts.push(catatan.clone());
        }
        Ok(json!({
            "qty": qty, "nama": nama, "harga": harga, "subtotal": harga * qty,
            "level": level, "extras": extras, "catatan": catatan,
            "note": parts.join(" · ")
        }))
    })?;
    rows.collect()
}

pub fn max_waktu_masak(conn: &Connection, order_id: i64) -> i64 {
    conn.query_row(
        "SELECT COALESCE(MAX(m.waktu_masak), 10) FROM order_items oi
           JOIN menu_items m ON m.id = oi.menu_id WHERE oi.order_id = ?1",
        params![order_id],
        |r| r.get(0),
    )
    .unwrap_or(10)
}

/// Tandai lunas. Idempoten: pemanggilan kedua (mis. webhook dikirim ulang) tidak berefek.
/// Return true kalau pesanan benar-benar baru berubah jadi lunas.
pub fn mark_paid(
    conn: &Connection,
    id: i64,
    metode: &str,
    bayar: i64,
    ref_: &str,
    uang: Option<i64>,
    kembalian: Option<i64>,
    kasir: Option<&str>,
) -> R<bool> {
    let tx = conn.unchecked_transaction()?;
    let (status, total, meja, metode_pesanan): (String, i64, String, String) = tx
        .query_row(
            "SELECT status, total, meja_no, metode FROM orders WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|_| not_found("pesanan tidak ditemukan"))?;

    // Fix A: metode pembayaran tidak boleh ditimpa diam-diam. Tanpa penjagaan ini
    // pesanan tunai bisa ditutup lewat "konfirmasi QRIS" tanpa ada uang masuk,
    // dan kolom metode ikut berubah sehingga jejaknya hilang.
    if metode_pesanan != metode {
        return Err(conflict(&format!(
            "pesanan ini dibayar {metode_pesanan}, bukan {metode}"
        )));
    }

    if status == "batal" || status == "kedaluwarsa" {
        return Err(conflict("pesanan sudah dibatalkan atau kedaluwarsa"));
    }
    if status != "menunggu_bayar" {
        return Ok(false);
    }
    if bayar < total {
        return Err(bad("jumlah bayar kurang dari total"));
    }
    let inserted = tx.execute(
        "INSERT OR IGNORE INTO payments(order_id, metode, jumlah, ref, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, metode, total, ref_, now()],
    )?;
    if inserted == 0 {
        return Ok(false);
    }
    tx.execute(
        "UPDATE orders SET status = 'baru', paid_at = ?2,
                uang_diterima = ?3, kembalian = ?4, kasir = ?5
         WHERE id = ?1",
        params![id, now(), uang, kembalian, kasir],
    )?;

    let k = kode(id);
    let label = if metode == "qris" { "QRIS" } else { "tunai" };
    let mut isi = format!(
        "{} via {label} sudah kami terima. Pesananmu langsung diteruskan ke dapur.",
        rupiah(total)
    );
    if let Some(kb) = kembalian {
        if kb > 0 {
            isi = format!("{} Kembalian {}.", isi, rupiah(kb));
        }
    }
    push(
        &tx, Some(id), "pembeli", "pembayaran_berhasil", "Pembayaran berhasil", &isi,
        Some(&format!("{id}:pembayaran_berhasil")),
    )?;
    let items = items_text(&tx, id)?;
    push(
        &tx, Some(id), "dapur", "pesanan_baru", &format!("Pesanan baru · Meja {meja}"),
        &format!("#{k} · {items}"),
        Some(&format!("{id}:dapur_baru")),
    )?;
    push(
        &tx, Some(id), "kasir", "pembayaran_masuk", &format!("Lunas · Meja {meja}"),
        &format!("#{k} · {} via {label}", rupiah(total)),
        Some(&format!("{id}:kasir_lunas")),
    )?;
    tx.commit()?;
    Ok(true)
}

/// baru -> dimasak -> siap -> selesai. Tiap perpindahan mengirim notifikasi ke pembeli.
pub fn advance_status(conn: &Connection, id: i64, to: &str) -> R<()> {
    let tx = conn.unchecked_transaction()?;
    let (status, meja): (String, String) = tx
        .query_row(
            "SELECT status, meja_no FROM orders WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| not_found("pesanan tidak ditemukan"))?;
    let ok = matches!(
        (status.as_str(), to),
        ("baru", "dimasak") | ("dimasak", "siap") | ("siap", "selesai")
    );
    if !ok {
        return Err(conflict(&format!("tidak bisa pindah dari {status} ke {to}")));
    }
    let k = kode(id);
    let n = now();
    match to {
        "dimasak" => {
            tx.execute("UPDATE orders SET status='dimasak', cooking_at=?2 WHERE id=?1", params![id, n])?;
            let est = max_waktu_masak(&tx, id);
            push(
                &tx, Some(id), "pembeli", "sedang_dimasak", "Pesananmu mulai dimasak",
                &format!("Pesanan #{k} sedang dimasak. Perkiraan siap sekitar {est} menit lagi."),
                Some(&format!("{id}:sedang_dimasak")),
            )?;
        }
        "siap" => {
            tx.execute("UPDATE orders SET status='siap', ready_at=?2 WHERE id=?1", params![id, n])?;
            push(
                &tx, Some(id), "pembeli", "pesanan_siap", "Pesanan selesai, siap diambil!",
                &format!(
                    "Pesanan #{k} sudah selesai dimasak. Pelayan akan mengantar ke Meja {meja}, atau kamu bisa ambil di counter."
                ),
                Some(&format!("{id}:pesanan_siap")),
            )?;
            push(
                &tx, Some(id), "kasir", "pesanan_siap", &format!("Siap antar · Meja {meja}"),
                &format!("#{k}"),
                Some(&format!("{id}:kasir_siap")),
            )?;
        }
        _ => {
            tx.execute("UPDATE orders SET status='selesai', served_at=?2 WHERE id=?1", params![id, n])?;
            push(
                &tx, Some(id), "pembeli", "pesanan_selesai", "Selamat menikmati!",
                &format!("Pesanan #{k} sudah sampai di Meja {meja}. Terima kasih sudah makan di sini."),
                Some(&format!("{id}:pesanan_selesai")),
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Batalkan pesanan. `oleh_staf` = true boleh membatalkan pesanan yang sudah lunas
/// (pengembalian uang diurus manual oleh kasir).
pub fn cancel(conn: &Connection, id: i64, oleh_staf: bool) -> R<()> {
    let tx = conn.unchecked_transaction()?;
    let status: String = tx
        .query_row("SELECT status FROM orders WHERE id = ?1", params![id], |r| r.get(0))
        .map_err(|_| not_found("pesanan tidak ditemukan"))?;
    let boleh = status == "menunggu_bayar"
        || (oleh_staf && (status == "baru" || status == "dimasak"));
    if !boleh {
        return Err(AppError(
            StatusCode::CONFLICT,
            "pesanan ini tidak bisa dibatalkan lagi".into(),
        ));
    }
    tx.execute(
        "UPDATE orders SET status='batal', cancelled_at=?2 WHERE id=?1",
        params![id, now()],
    )?;
    let k = kode(id);
    let sudah_bayar = status != "menunggu_bayar";
    let isi = if sudah_bayar {
        format!("Pesanan #{k} dibatalkan oleh kasir. Silakan ke kasir untuk pengembalian dana.")
    } else {
        format!("Pesanan #{k} dibatalkan.")
    };
    push(
        &tx, Some(id), "pembeli", "pesanan_dibatalkan", "Pesanan dibatalkan", &isi,
        Some(&format!("{id}:pesanan_dibatalkan")),
    )?;
    tx.commit()?;
    Ok(())
}

pub fn estimasi_menit(status: &str, est: i64, cooking_at: Option<i64>) -> i64 {
    match status {
        "baru" => est,
        "dimasak" => {
            let lewat = (now() - cooking_at.unwrap_or(now())) / 60;
            (est - lewat).max(1)
        }
        _ => 0,
    }
}
