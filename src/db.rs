use crate::classify::{classify, is_usable_title};
use crate::models::{IncomingListing, Listing, ListQuery};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPool;
use sqlx::PgPoolOptions;
use sqlx::Row;
use std::collections::HashSet;

pub type DbPool = PgPool;

pub async fn open(database_url: &str) -> Result<DbPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(8)
        .connect(database_url)
        .await
}

pub async fn init(pool: &DbPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS listings (
            id TEXT PRIMARY KEY,
            source TEXT NOT NULL,
            source_url TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL,
            company TEXT,
            location TEXT NOT NULL,
            county TEXT,
            listing_type TEXT NOT NULL,
            description TEXT NOT NULL,
            published_at TEXT,
            scraped_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_listings_type ON listings(listing_type)")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_listings_published ON listings(published_at DESC)")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_listings_county ON listings(county)")
        .execute(pool)
        .await?;

    Ok(())
}

pub fn listing_id(url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    hex(&hasher.finalize())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub async fn upsert(pool: &DbPool, item: &IncomingListing) -> Result<bool, sqlx::Error> {
    let id = listing_id(&item.source_url);
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        r#"
        INSERT INTO listings (
            id, source, source_url, title, company, location, county,
            listing_type, description, published_at, scraped_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        ON CONFLICT(source_url) DO UPDATE SET
            title = excluded.title,
            company = excluded.company,
            location = excluded.location,
            county = excluded.county,
            listing_type = excluded.listing_type,
            description = excluded.description,
            published_at = COALESCE(excluded.published_at, listings.published_at),
            scraped_at = excluded.scraped_at
        "#,
    )
    .bind(&id)
    .bind(&item.source)
    .bind(&item.source_url)
    .bind(&item.title)
    .bind(&item.company)
    .bind(&item.location)
    .bind(&item.county)
    .bind(item.listing_type.as_str())
    .bind(&item.description)
    .bind(&item.published_at)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

fn map_listing(row: &sqlx::postgres::PgRow) -> Result<Listing, sqlx::Error> {
    Ok(Listing {
        id: row.try_get(0)?,
        source: row.try_get(1)?,
        source_url: row.try_get(2)?,
        title: row.try_get(3)?,
        company: row.try_get(4)?,
        location: row.try_get(5)?,
        county: row.try_get(6)?,
        listing_type: row.try_get(7)?,
        description: row.try_get(8)?,
        published_at: row.try_get(9)?,
        scraped_at: row.try_get(10)?,
    })
}

const COLS: &str = "id, source, source_url, title, company, location, county, listing_type, description, published_at, scraped_at";

pub async fn get_by_id(pool: &DbPool, id: &str) -> Result<Option<Listing>, sqlx::Error> {
    sqlx::query(&format!("SELECT {COLS} FROM listings WHERE id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .map(map_listing)
        .transpose()
}

pub async fn search(pool: &DbPool, q: &ListQuery) -> Result<(i64, Vec<Listing>), sqlx::Error> {
    let page = q.page.unwrap_or(1).max(1);
    let limit = q.limit.unwrap_or(24).clamp(1, 100);
    let offset = (page - 1) * limit;

    let mut where_sql = String::from("WHERE 1=1");
    let mut vals: Vec<String> = Vec::new();
    let mut param_idx = 1;

    if let Some(t) = q.listing_type.as_deref().and_then(crate::models::ListingType::parse) {
        where_sql.push_str(&format!(" AND listing_type = ${}", param_idx));
        vals.push(t.as_str().to_string());
        param_idx += 1;
    }
    if let Some(county) = q.county.as_deref().filter(|s| !s.trim().is_empty()) {
        where_sql.push_str(&format!(" AND (county LIKE ${} OR location LIKE ${})", param_idx, param_idx + 1));
        vals.push(format!("%{county}%"));
        vals.push(format!("%{county}%"));
        param_idx += 2;
    }
    if let Some(source) = q.source.as_deref().filter(|s| !s.trim().is_empty()) {
        where_sql.push_str(&format!(" AND source = ${}", param_idx));
        vals.push(source.to_string());
        param_idx += 1;
    }
    if let Some(query) = q.q.as_deref().filter(|s| !s.trim().is_empty()) {
        where_sql.push_str(&format!(" AND (title LIKE ${} OR company LIKE ${} OR description LIKE ${})", param_idx, param_idx + 1, param_idx + 2));
        let like = format!("%{query}%");
        vals.push(like.clone());
        vals.push(like.clone());
        vals.push(like);
        param_idx += 3;
    }

    let count_sql = format!("SELECT COUNT(*) FROM listings {where_sql}");
    let mut count_query = sqlx::query(&count_sql);
    for val in &vals {
        count_query = count_query.bind(val);
    }
    let total: i64 = count_query.fetch_one(pool).await?.try_get(0)?;

    let list_sql = format!(
        "SELECT {COLS} FROM listings {where_sql} ORDER BY COALESCE(published_at, scraped_at) DESC LIMIT ${} OFFSET ${}",
        param_idx, param_idx + 1
    );
    let mut list_query = sqlx::query(&list_sql);
    for val in &vals {
        list_query = list_query.bind(val);
    }
    list_query = list_query.bind(limit as i64);
    list_query = list_query.bind(offset as i64);
    let rows = list_query.fetch_all(pool).await?;
    let items: Result<Vec<Listing>, sqlx::Error> = rows.iter().map(map_listing).collect();
    Ok((total, unique_listings(items?, limit as usize)))
}

fn unique_listings(items: Vec<Listing>, limit: usize) -> Vec<Listing> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in items {
        let key = format!(
            "{}|{}",
            item.title.to_ascii_lowercase(),
            item.company.clone().unwrap_or_default().to_ascii_lowercase()
        );
        if seen.insert(key) {
            out.push(item);
            if out.len() >= limit {
                break;
            }
        }
    }
    out
}

pub async fn latest_by_type(pool: &DbPool, listing_type: &str, limit: i64) -> Result<Vec<Listing>, sqlx::Error> {
    let rows = sqlx::query(&format!(
        "SELECT {COLS} FROM listings WHERE listing_type = $1 ORDER BY COALESCE(published_at, scraped_at) DESC LIMIT $2"
    ))
    .bind(listing_type)
    .bind(limit * 3)
    .fetch_all(pool)
    .await?;
    let items: Result<Vec<Listing>, sqlx::Error> = rows.iter().map(map_listing).collect();
    Ok(unique_listings(items?, limit as usize))
}

pub async fn latest_matching(pool: &DbPool, like: &str, limit: i64) -> Result<Vec<Listing>, sqlx::Error> {
    let pattern = format!("%{like}%");
    let rows = sqlx::query(&format!(
        "SELECT {COLS} FROM listings WHERE location LIKE $1 OR county LIKE $1 ORDER BY COALESCE(published_at, scraped_at) DESC LIMIT $2"
    ))
    .bind(&pattern)
    .bind(limit * 3)
    .fetch_all(pool)
    .await?;
    let items: Result<Vec<Listing>, sqlx::Error> = rows.iter().map(map_listing).collect();
    Ok(unique_listings(items?, limit as usize))
}

pub async fn featured(pool: &DbPool) -> Result<Option<Listing>, sqlx::Error> {
    for listing_type in ["internship", "attachment"] {
        let found = sqlx::query(&format!(
            "SELECT {COLS} FROM listings WHERE listing_type = $1
             ORDER BY COALESCE(published_at, scraped_at) DESC LIMIT 1"
        ))
        .bind(listing_type)
        .fetch_optional(pool)
        .await?
        .map(map_listing)
        .transpose()?;
        if found.is_some() {
            return Ok(found);
        }
    }
    sqlx::query(&format!(
        "SELECT {COLS} FROM listings ORDER BY COALESCE(published_at, scraped_at) DESC LIMIT 1"
    ))
    .fetch_optional(pool)
    .await?
    .map(map_listing)
    .transpose()
}

pub async fn count_type(pool: &DbPool, listing_type: &str) -> Result<i64, sqlx::Error> {
    let result = sqlx::query("SELECT COUNT(*) FROM listings WHERE listing_type = $1")
        .bind(listing_type)
        .fetch_one(pool)
        .await?;
    result.try_get(0)
}

pub async fn count_all(pool: &DbPool) -> Result<i64, sqlx::Error> {
    let result = sqlx::query("SELECT COUNT(*) FROM listings")
        .fetch_one(pool)
        .await?;
    result.try_get(0)
}

pub async fn refresh_types(pool: &DbPool) -> Result<usize, sqlx::Error> {
    let rows = sqlx::query("SELECT id, title, description FROM listings")
        .fetch_all(pool)
        .await?;
    let mut touched = 0;
    for row in rows {
        let id: String = row.try_get(0)?;
        let title: String = row.try_get(1)?;
        let description: String = row.try_get(2)?;

        if !is_usable_title(&title) {
            sqlx::query("DELETE FROM listings WHERE id = $1")
                .bind(&id)
                .execute(pool)
                .await?;
            touched += 1;
            continue;
        }
        let listing_type = classify(&title, &description);
        sqlx::query("UPDATE listings SET listing_type = $1 WHERE id = $2")
            .bind(listing_type.as_str())
            .bind(&id)
            .execute(pool)
            .await?;
        touched += 1;
    }
    Ok(touched)
}
