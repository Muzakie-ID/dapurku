"""Simulator payment gateway QRIS.

Mengirim webhook bertanda tangan HMAC ke server Rust, persis seperti yang akan
dilakukan payment gateway sungguhan.

  python -m worker.qris_sim DPK-0142            # bayar satu pesanan
  python -m worker.qris_sim --watch --delay 8   # otomatis bayar semua pesanan QRIS setelah 8 detik
"""
import argparse
import hashlib
import hmac
import json
import os
import time
import urllib.error
import urllib.request

from .common import connect, kode, now

SERVER = os.environ.get("SERVER_URL", "http://localhost:8080")
SECRET = os.environ.get("WEBHOOK_SECRET", "dev-secret-ganti-di-produksi")


def kirim_webhook(kode_pesanan, jumlah, ref=None):
    body = json.dumps({
        "kode": kode_pesanan, "amount": jumlah, "status": "PAID",
        "ref": ref or f"SIM{int(time.time() * 1000)}",
    }).encode()
    sig = hmac.new(SECRET.encode(), body, hashlib.sha256).hexdigest()
    req = urllib.request.Request(SERVER + "/api/webhook/qris", data=body, method="POST",
                                 headers={"Content-Type": "application/json", "X-Signature": sig})
    try:
        with urllib.request.urlopen(req, timeout=8) as r:
            return r.status, json.loads(r.read() or b"{}")
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read() or b"{}")


def bayar_satu(kode_pesanan):
    conn = connect()
    try:
        oid = int(kode_pesanan.upper().replace("#", "").replace("DPK-", ""))
    except ValueError:
        raise SystemExit("format kode: DPK-0142")
    row = conn.execute("SELECT total, status FROM orders WHERE id=?", (oid,)).fetchone()
    if not row:
        raise SystemExit("pesanan tidak ditemukan")
    print(kirim_webhook(kode(oid), row["total"], f"SIM-{oid}-{now()}"))


def watch(delay):
    print(f"Mengawasi pesanan QRIS baru, otomatis dibayar setelah {delay} detik. Ctrl+C untuk berhenti.")
    conn = connect()
    while True:
        rows = conn.execute(
            "SELECT id, total FROM orders WHERE status='menunggu_bayar' AND metode='qris' AND created_at <= ?",
            (now() - delay,),
        ).fetchall()
        for r in rows:
            print(kode(r["id"]), kirim_webhook(kode(r["id"]), r["total"], f"SIM-{r['id']}"), flush=True)
        time.sleep(2)


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("kode", nargs="?")
    ap.add_argument("--watch", action="store_true")
    ap.add_argument("--delay", type=int, default=8)
    a = ap.parse_args()
    if a.watch:
        watch(a.delay)
    elif a.kode:
        bayar_satu(a.kode)
    else:
        ap.print_help()
