mod api_public;
mod api_staff;
mod auth;
mod db;
mod notify;
mod orders;
mod sse;
mod util;

use axum::{
    routing::{get, post, put},
    Router,
};
use rusqlite::Connection;
use std::{net::SocketAddr, sync::{Arc, Mutex}};
use tower_http::services::ServeDir;

pub struct Config {
    pub web_dir: String,
    pub sim: bool,
    pub webhook_secret: String,
    pub pin_kasir: String,
    pub pin_dapur: String,
    pub pin_admin: String,
    pub tz: i64, // menit di atas UTC, WIB = 420
    pub base_url: String,
}

pub struct App {
    pub db: Mutex<Connection>,
    pub cfg: Config,
}

fn env(k: &str, d: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| d.to_string())
}

#[tokio::main]
async fn main() {
    let db_path = env("DATABASE_PATH", "../data/dapurku.db");
    let port: u16 = env("PORT", "8080").parse().unwrap_or(8080);
    let cfg = Config {
        web_dir: env("WEB_DIR", "../web"),
        sim: env("PAYMENT_MODE", "sim") == "sim",
        webhook_secret: env("WEBHOOK_SECRET", "dev-secret-ganti-di-produksi"),
        pin_kasir: env("KASIR_PIN", "1111"),
        pin_dapur: env("DAPUR_PIN", "2222"),
        pin_admin: env("ADMIN_PIN", "9999"),
        tz: env("TZ_OFFSET_MIN", "420").parse().unwrap_or(420),
        base_url: env("BASE_URL", ""),
    };
    if cfg.pin_admin == "9999" || cfg.webhook_secret.starts_with("dev-secret") {
        eprintln!("PERINGATAN: PIN/secret masih default. Ganti lewat env sebelum dipakai sungguhan.");
    }
    if cfg.sim {
        eprintln!("Mode pembayaran: SIMULASI (tombol 'sudah bayar' aktif). Set PAYMENT_MODE=live untuk mematikan.");
    }

    let conn = db::open(&db_path).unwrap_or_else(|e| panic!("gagal membuka database {db_path}: {e}"));
    let web_dir = cfg.web_dir.clone();
    let state = Arc::new(App { db: Mutex::new(conn), cfg });

    use api_public as p;
    use api_staff as s;
    let app = Router::new()
        // publik
        .route("/api/config", get(p::config))
        .route("/api/menu", get(p::menu))
        .route("/api/meja", get(p::meja_list))
        .route("/api/meja/:no/qr.svg", get(p::meja_qr))
        .route("/api/orders", post(p::create_order))
        .route("/api/orders/:token", get(p::get_order))
        .route("/api/orders/:token/qr.svg", get(p::order_qr))
        .route("/api/orders/:token/events", get(p::order_events))
        .route("/api/orders/:token/simulate-pay", post(p::simulate_pay))
        .route("/api/orders/:token/panggil", post(p::panggil))
        .route("/api/orders/:token/batal", post(p::customer_cancel))
        .route("/api/webhook/qris", post(p::webhook_qris))
        // login
        .route("/api/login", post(p::login))
        .route("/api/logout", post(p::logout))
        .route("/api/me", get(p::me))
        // staf
        .route("/api/staff/events", get(s::staff_events))
        .route("/api/staff/dapur", get(s::dapur))
        .route("/api/staff/kasir", get(s::kasir))
        .route("/api/staff/orders", get(s::orders_list))
        .route("/api/staff/orders/:id", get(s::order_detail))
        .route("/api/staff/orders/:id/status", post(s::set_status))
        .route("/api/staff/orders/:id/bayar-tunai", post(s::bayar_tunai))
        .route("/api/staff/orders/:id/konfirmasi-qris", post(s::konfirmasi_qris))
        .route("/api/staff/orders/:id/batal", post(s::batal))
        .route("/api/staff/meja/:no/kosongkan", post(s::kosongkan))
        // admin
        .route("/api/admin/ringkasan", get(s::ringkasan))
        .route("/api/admin/orders", get(s::admin_orders))
        .route("/api/admin/menu", get(s::admin_menu).post(s::menu_create))
        .route("/api/admin/menu/:id", put(s::menu_update).delete(s::menu_hapus))
        .route("/api/admin/menu/:id/tersedia", post(s::menu_tersedia))
        .route("/api/admin/meja", get(s::admin_meja).post(s::meja_tambah))
        .route("/api/admin/meja/:no/nonaktif", post(s::meja_nonaktif))
        .route("/api/admin/laporan", get(s::laporan))
        .route("/api/admin/laporan.csv", get(s::laporan_csv))
        .route("/api/admin/settings", get(s::settings_get).put(s::settings_put))
        .with_state(state)
        .fallback_service(ServeDir::new(web_dir).append_index_html_on_directories(true));

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    eprintln!("Dapurku berjalan di http://localhost:{port}  (database: {db_path})");
    let listener = tokio::net::TcpListener::bind(addr).await.expect("port sudah dipakai?");
    axum::serve(listener, app).await.expect("server berhenti");
}
