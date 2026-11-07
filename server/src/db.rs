use rusqlite::{params, Connection};
use std::time::Duration;

pub fn open(path: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_millis(5000))?;
    // WAL supaya worker Python bisa membaca/menulis bersamaan dengan server.
    let _mode: String = conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
    conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=NORMAL;")?;
    conn.execute_batch(include_str!("../../schema/schema.sql"))?;
    seed_if_empty(&conn)?;
    Ok(conn)
}

fn seed_if_empty(conn: &Connection) -> rusqlite::Result<()> {
    let defaults = [
        ("nama_resto", "Dapurku"),
        ("merchant_nmid", "SIMULASI"),
        ("bayar_menit", "15"),   // batas bayar QRIS
        ("tunai_menit", "30"),   // batas datang ke kasir untuk bayar tunai
        ("target_masak", "15"),  // target waktu masak (menit)
        ("telat_menit", "15"),   // lewat ini pesanan dianggap terlambat
    ];
    for (k, v) in defaults {
        conn.execute(
            "INSERT OR IGNORE INTO settings(key, value) VALUES (?1, ?2)",
            params![k, v],
        )?;
    }

    let n_menu: i64 = conn.query_row("SELECT COUNT(*) FROM menu_items", [], |r| r.get(0))?;
    if n_menu == 0 {
        // (nama, kategori, harga, deskripsi, pedas, waktu_masak, tersedia)
        let menu = [
            ("Nasi Goreng Kampung", "Nasi", 18000, "Nasi goreng bumbu kampung dengan telur dan kerupuk.", 0, 10, 1),
            ("Ayam Geprek", "Nasi", 17000, "Ayam crispy digeprek sambal bawang, nasi putih, lalapan.", 1, 12, 1),
            ("Mie Ayam Bakso", "Mie", 16000, "Mie ayam dengan bakso dan kuah kaldu.", 0, 9, 1),
            ("Soto Ayam", "Kuah", 15000, "Soto ayam kuah bening dengan nasi.", 0, 8, 1),
            ("Es Teh Manis", "Minuman", 5000, "Teh manis dingin.", 0, 3, 1),
            ("Es Jeruk Peras", "Minuman", 8000, "Jeruk peras segar dengan es.", 0, 3, 0),
        ];
        for (i, m) in menu.iter().enumerate() {
            conn.execute(
                "INSERT INTO menu_items(nama,kategori,harga,deskripsi,pedas,waktu_masak,tersedia,urutan)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![m.0, m.1, m.2, m.3, m.4, m.5, m.6, i as i64],
            )?;
        }
        // Tambahan khusus Ayam Geprek (id 2)
        conn.execute("INSERT INTO menu_extras(menu_id,nama,harga) VALUES (2,'Telur ceplok',4000)", [])?;
        conn.execute("INSERT INTO menu_extras(menu_id,nama,harga) VALUES (2,'Keju leleh',5000)", [])?;
    }

    let n_meja: i64 = conn.query_row("SELECT COUNT(*) FROM meja", [], |r| r.get(0))?;
    if n_meja == 0 {
        for i in 1..=12 {
            conn.execute("INSERT INTO meja(no) VALUES (?1)", params![format!("{:02}", i)])?;
        }
    }
    Ok(())
}
