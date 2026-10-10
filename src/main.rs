mod api;
mod classify;
mod db;
mod models;
mod scrapers;

use api::AppState;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::info;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "kazibox=info,tower_http=info".into()),
        )
        .init();

    let db_path = std::env::var("KAZIBOX_DB").unwrap_or_else(|_| "kazibox.db".into());
    let pool = db::open(&db_path).expect("open sqlite");
    db::init(&pool).expect("init sqlite");
    if let Ok(n) = db::refresh_types(&pool) {
        info!(rows = n, "reclassified stored listings");
    }

    let state = AppState {
        pool: pool.clone(),
        scraping: Arc::new(Mutex::new(false)),
    };

    let scrape_pool = pool.clone();
    let scrape_lock = state.scraping.clone();
    tokio::spawn(async move {
        loop {
            {
                let mut busy = scrape_lock.lock().await;
                if !*busy {
                    *busy = true;
                    drop(busy);
                    info!("starting scheduled scrape");
                    let reports = scrapers::run_all(&scrape_pool).await;
                    info!(?reports, "scheduled scrape finished");
                    *scrape_lock.lock().await = false;
                }
            }
            tokio::time::sleep(Duration::from_secs(30 * 60)).await;
        }
    });

    let frontend_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../frontend/dist");
    let app = api::router(state, frontend_dir);
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .expect("invalid PORT");
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .expect("bind http");
    info!("254jobs API listening on http://0.0.0.0:{port}");
    axum::serve(listener, app).await.expect("server");
}
