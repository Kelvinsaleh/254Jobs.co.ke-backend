use crate::db::{self, DbPool};
use crate::models::{HomeResponse, ListQuery, ListResponse, Listing, ScrapeReport, Stats};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
pub struct AppState {
    pub pool: DbPool,
    pub scraping: Arc<Mutex<bool>>,
}

pub fn router(state: AppState, frontend_dir: std::path::PathBuf) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let api = Router::new()
        .route("/api/health", get(health))
        .route("/api/home", get(home))
        .route("/api/listings", get(list_listings))
        .route("/api/listings/{id}", get(get_listing))
        .route("/api/scrape", post(trigger_scrape))
        .with_state(state)
        .layer(cors);

    if frontend_dir.join("index.html").exists() {
        let index = ServeFile::new(frontend_dir.join("index.html"));
        api.fallback_service(ServeDir::new(frontend_dir).not_found_service(index))
    } else {
        api
    }
}

#[derive(Serialize)]
struct Health {
    ok: bool,
    listings: i64,
}

async fn health(State(state): State<AppState>) -> Json<Health> {
    let listings = db::count_all(&state.pool).unwrap_or(0);
    Json(Health { ok: true, listings })
}

async fn home(State(state): State<AppState>) -> Result<Json<HomeResponse>, StatusCode> {
    let featured = db::featured(&state.pool).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let internships =
        db::latest_by_type(&state.pool, "internship", 18).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let attachments =
        db::latest_by_type(&state.pool, "attachment", 18).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let jobs = db::latest_by_type(&state.pool, "job", 18).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let nairobi =
        db::latest_matching(&state.pool, "Nairobi", 18).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let remote =
        db::latest_matching(&state.pool, "Remote", 18).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let stats = Stats {
        jobs: db::count_type(&state.pool, "job").unwrap_or(0),
        internships: db::count_type(&state.pool, "internship").unwrap_or(0),
        attachments: db::count_type(&state.pool, "attachment").unwrap_or(0),
        total: db::count_all(&state.pool).unwrap_or(0),
    };
    Ok(Json(HomeResponse {
        featured,
        internships,
        attachments,
        jobs,
        nairobi,
        remote,
        stats,
    }))
}

async fn list_listings(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<ListResponse>, StatusCode> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(24).clamp(1, 100);
    let (total, items) = db::search(&state.pool, &query).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ListResponse {
        total,
        page,
        limit,
        items,
    }))
}

async fn get_listing(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Listing>, StatusCode> {
    db::get_by_id(&state.pool, &id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

#[derive(Serialize)]
struct ScrapeResponse {
    running: bool,
    reports: Vec<ScrapeReport>,
}

async fn trigger_scrape(State(state): State<AppState>) -> Json<ScrapeResponse> {
    let mut guard = state.scraping.lock().await;
    if *guard {
        return Json(ScrapeResponse {
            running: true,
            reports: vec![],
        });
    }
    *guard = true;
    drop(guard);
    let reports = crate::scrapers::run_all(&state.pool).await;
    *state.scraping.lock().await = false;
    Json(ScrapeResponse {
        running: false,
        reports,
    })
}
