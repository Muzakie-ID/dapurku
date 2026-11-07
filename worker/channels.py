"""Kanal pengiriman notifikasi ke luar aplikasi.

Notifikasi di halaman status pembeli ditangani langsung oleh server Rust (SSE).
Modul ini meneruskan notifikasi yang sama ke kanal lain. Tambah kanal baru
(WhatsApp, Telegram, email) dengan membuat class dengan method send(n) dan
mendaftarkannya di enabled_channels().
"""
import json
import os
import urllib.request


class ConsoleChannel:
    name = "console"

    def send(self, n):
        print(f"[notif] #{n['kode']} Meja {n['meja']} · {n['judul']}: {n['isi']}", flush=True)


class WebhookChannel:
    """Kirim JSON ke URL apa pun (misal gateway WhatsApp atau Zapier/n8n)."""
    name = "webhook"

    def __init__(self, url, token=""):
        self.url, self.token = url, token

    def send(self, n):
        body = json.dumps(n).encode()
        req = urllib.request.Request(self.url, data=body, method="POST",
                                     headers={"Content-Type": "application/json"})
        if self.token:
            req.add_header("Authorization", "Bearer " + self.token)
        with urllib.request.urlopen(req, timeout=8) as r:
            if r.status >= 300:
                raise RuntimeError(f"webhook membalas {r.status}")


def enabled_channels():
    chans = []
    if os.environ.get("NOTIFY_CONSOLE", "1") == "1":
        chans.append(ConsoleChannel())
    url = os.environ.get("NOTIFY_WEBHOOK_URL")
    if url:
        chans.append(WebhookChannel(url, os.environ.get("NOTIFY_WEBHOOK_TOKEN", "")))
    return chans
