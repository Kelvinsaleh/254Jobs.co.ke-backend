pub mod http;
pub mod jobweb;
pub mod myjobmag;
pub mod reliefweb;

use crate::db::DbPool;
use crate::models::{IncomingListing, ScrapeReport};
use tracing::{info, warn};

pub async fn run_all(pool: &DbPool) -> Vec<ScrapeReport> {
    let mut reports = Vec::new();
    reports.push(run_source(pool, "jobweb", jobweb::fetch).await);
    reports.push(run_source(pool, "myjobmag", myjobmag::fetch).await);
    reports.push(run_source(pool, "reliefweb", reliefweb::fetch).await);
    reports
}

async fn run_source<F, Fut>(pool: &DbPool, name: &str, fetch: F) -> ScrapeReport
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<Vec<IncomingListing>, String>>,
{
    match fetch().await {
        Ok(items) => {
            let fetched = items.len();
            let mut upserted = 0;
            for item in items {
                match crate::db::upsert(pool, &item) {
                    Ok(_) => upserted += 1,
                    Err(err) => warn!(source = name, error = %err, "upsert failed"),
                }
            }
            info!(source = name, fetched, upserted, "scrape complete");
            ScrapeReport {
                source: name.to_string(),
                fetched,
                upserted,
                error: None,
            }
        }
        Err(error) => {
            warn!(source = name, error = %error, "scrape failed");
            ScrapeReport {
                source: name.to_string(),
                fetched: 0,
                upserted: 0,
                error: Some(error),
            }
        }
    }
}
