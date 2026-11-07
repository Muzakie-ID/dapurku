"""Isi database dengan pesanan contoh (sama dengan data di desain) supaya semua layar ada isinya.
Jalankan server sekali dulu (membuat tabel dan menu), lalu:  python -m worker.seed_demo
"""
from .common import connect, now
import secrets

MENU = {}


def tambah(conn, meja, metode, status, menit_lalu, items, bayar=True, siap_lalu=None):
    t = now() - menit_lalu * 60
    lines = []
    for nama, qty, level, extras, catatan in items:
        m = MENU[nama]
        lines.append((m["id"], nama, m["harga"] + sum(h for _, h in extras), qty, level, ", ".join(n for n, _ in extras), catatan))
    total = sum(l[2] * l[3] for l in lines)
    paid = t if bayar else None
    cur = conn.execute(
        """INSERT INTO orders(token,meja_no,metode,status,total,created_at,expires_at,paid_at,cooking_at,ready_at,served_at,dibersihkan)
           VALUES (?,?,?,?,?,?,?,?,?,?,?,0)""",
        (secrets.token_hex(8), meja, metode, status, total, t, t + 900, paid,
         t + 60 if status in ("dimasak", "siap", "selesai") else None,
         now() - siap_lalu * 60 if siap_lalu is not None else None,
         now() - (siap_lalu or 0) * 60 + 300 if status == "selesai" else None),
    )
    oid = cur.lastrowid
    for l in lines:
        conn.execute("INSERT INTO order_items(order_id,menu_id,nama,harga,qty,level_pedas,extras,catatan) VALUES (?,?,?,?,?,?,?,?)", (oid, *l))
    if bayar:
        conn.execute("INSERT INTO payments(order_id,metode,jumlah,ref,created_at) VALUES (?,?,?,?,?)", (oid, metode, total, f"DEMO-{oid}", t))


def main():
    conn = connect()
    for r in conn.execute("SELECT id, nama, harga FROM menu_items"):
        MENU[r["nama"]] = {"id": r["id"], "harga": r["harga"]}
    if "Nasi Goreng Kampung" not in MENU:
        raise SystemExit("Menu belum ada. Jalankan server Rust sekali dulu.")
    if conn.execute("SELECT COUNT(*) FROM orders").fetchone()[0]:
        raise SystemExit("Database sudah berisi pesanan, seed dibatalkan.")
    telur = ("Telur ceplok", 4000)
    conn.execute("BEGIN")
    # sudah selesai (untuk laporan & denah 'makan')
    tambah(conn, "07", "qris", "selesai", 40, [("Nasi Goreng Kampung", 2, None, [], ""), ("Es Teh Manis", 2, None, [], "")], siap_lalu=25)
    for i in range(6):
        tambah(conn, f"{(i % 4) + 1:02d}", "qris" if i % 3 else "tunai", "selesai", 150 + i * 20, [("Ayam Geprek", 1, 2, [], ""), ("Es Teh Manis", 1, None, [], "")], siap_lalu=120 + i * 20)
    # layar dapur
    tambah(conn, "03", "qris", "dimasak", 6, [("Mie Ayam Bakso", 2, None, [], "")])
    tambah(conn, "08", "qris", "dimasak", 10, [("Mie Ayam Bakso", 1, None, [], ""), ("Ayam Geprek", 1, 0, [], "")])
    tambah(conn, "02", "qris", "dimasak", 17, [("Soto Ayam", 3, None, [], "Nasi dipisah")])
    tambah(conn, "05", "qris", "baru", 1, [("Nasi Goreng Kampung", 2, None, [], "1 tidak pedas")])
    tambah(conn, "11", "qris", "baru", 0, [("Ayam Geprek", 1, 3, [telur], ""), ("Es Teh Manis", 1, None, [], "")])
    tambah(conn, "06", "tunai", "siap", 8, [("Soto Ayam", 1, None, [], "")], siap_lalu=2)
    # belum bayar
    tambah(conn, "09", "tunai", "menunggu_bayar", 3, [("Mie Ayam Bakso", 2, None, [], ""), ("Ayam Geprek", 1, 2, [], ""), ("Es Teh Manis", 3, None, [], "")], bayar=False)
    tambah(conn, "12", "qris", "menunggu_bayar", 2, [("Soto Ayam", 1, None, [], ""), ("Es Jeruk Peras", 1, None, [], "")], bayar=False)
    conn.execute("COMMIT")
    print("Selesai: pesanan contoh ditambahkan.")


if __name__ == "__main__":
    main()
