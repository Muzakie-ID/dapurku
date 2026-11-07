# Dapurku — pesan dari meja, bayar QRIS

Implementasi desain "Dapurku QR Order v2": 4 layar pelanggan (menu, detail, bayar QRIS, status),
layar dapur, layar kasir, dashboard admin. Database SQLite, notifikasi realtime ke pembeli.

```
schema.sql   skema database (dipakai server Rust dan worker Python)
server/      Rust (Axum + SQLite): API, auth staf, webhook QRIS, SSE notifikasi, QR meja
web/         HTML/CSS/JS statis yang disajikan server (tanpa build step)
worker/      Python (stdlib saja): kedaluwarsa bayar, pengingat, peringatan telat,
             teruskan notifikasi ke kanal luar, simulator payment gateway, seed demo
```

## Menjalankan

```bash
# 1. Server (butuh Rust 1.75+)
cd server
KASIR_PIN=1111 DAPUR_PIN=2222 ADMIN_PIN=9999 cargo run --release
# buka http://localhost:8080/?meja=05

# 2. Worker (terminal lain, dari folder proyek)
python -m worker.main

# 3. Opsional
python -m worker.seed_demo               # isi pesanan contoh seperti di desain
python -m worker.qris_sim --watch        # simulasi gateway: bayar otomatis tiap pesanan QRIS
```

Staf login di `/login.html` (PIN per peran). Admin: `/admin.html`, kasir: `/kasir.html`, dapur: `/dapur.html`.

## Alur dan notifikasi ke pembeli

| Kejadian | Notifikasi ke pembeli | Sumber |
|---|---|---|
| Pesanan dibuat | Pesanan diterima | server |
| QRIS tinggal 5 menit | Segera selesaikan pembayaran | worker |
| Pembayaran lunas (QRIS/tunai) | Pembayaran berhasil | server |
| Dapur mulai masak | Pesananmu mulai dimasak (+estimasi) | server |
| Dapur tandai siap | Pesanan selesai, siap diambil | server |
| Diantar | Selamat menikmati | server |
| Lewat batas waktu masak | Maaf, pesananmu agak lama | worker |
| Tidak dibayar tepat waktu | Waktu bayar habis | worker |
| Dibatalkan kasir/pembeli | Pesanan dibatalkan | server |
| Pembeli tekan "Panggil pelayan" | Pelayan dipanggil (dan kasir diberi tahu) | server |

Semua ditulis ke tabel `notifications`. Halaman pembeli menerimanya lewat SSE
(`/api/orders/:token/events`): toast, bunyi, dan notifikasi browser kalau diizinkan.
Kondisi: notifikasi muncul selama halaman masih terbuka (tab boleh di latar belakang).
Untuk notifikasi saat tab ditutup, tambahkan Web Push atau kanal luar. Worker sudah
meneruskan tiap notifikasi pembeli ke `NOTIFY_WEBHOOK_URL` (mis. gateway WhatsApp atau n8n);
tambah kanal baru di `worker/channels.py`.

## Pembayaran QRIS

Saat ini **simulasi**: QR berisi data pesanan, bukan QRIS sungguhan, dan tombol "demo: sudah bayar"
aktif selama `PAYMENT_MODE=sim`. Untuk produksi: buat QR dinamis lewat payment gateway/PJP berizin
dan arahkan webhook-nya ke `POST /api/webhook/qris`
(body `{"kode","ref","amount","status":"PAID"}`, header `X-Signature` = HMAC-SHA256 hex dari body
dengan `WEBHOOK_SECRET`). Webhook idempoten, jadi kiriman ganda aman. Lalu set `PAYMENT_MODE=live`.

## Keamanan

- Ganti semua PIN dan `WEBHOOK_SECRET`; server memberi peringatan kalau masih default.
- Pelanggan mengakses pesanannya lewat token acak di URL, bukan nomor urut.
- Harga dihitung ulang di server; teks dari pengguna selalu di-escape di semua layar.
- Pasang di belakang HTTPS (cookie login `HttpOnly`, `SameSite=Lax`; tambahkan `Secure` di reverse proxy bila perlu).

## Perbedaan dari desain

- Chip kategori diambil dari data menu (Semua, Nasi, Mie, Kuah, Minuman), bukan daftar tetap.
- Tambahan: lembar keranjang (ubah jumlah, pilih QRIS/tunai), banner pesanan aktif, riwayat notifikasi,
  halaman login staf, struk, halaman cetak QR meja.
- Menu samping admin "Pengguna" tidak dibuat (login memakai PIN per peran). "Pengaturan" berisi
  batas waktu bayar/masak.
- Foto menu: isi URL di form "Ubah menu"; tanpa URL tampil placeholder [FOTO] seperti desain.
