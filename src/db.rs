use crate::classify::{classify, is_usable_title};
use crate::models::{IncomingListing, Listing, ListQuery};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub type DbPool = Pool<SqliteConnectionManager>;

pub fn open(path: &str) -> Result<DbPool, r2d2::Error> {
    let manager = SqliteConnectionManager::file(path);
    Pool::builder().max_size(8).build(manager)
}

pub fn init(pool: &DbPool) -> Result<(), rusqlite::Error> {
    let conn = pool.get().expect("db pool");
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
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
        CREATE INDEX IF NOT EXISTS idx_listings_type ON listings(listing_type);
        CREATE INDEX IF NOT EXISTS idx_listings_published ON listings(published_at DESC);
        CREATE INDEX IF NOT EXISTS idx_listings_county ON listings(county);
        "#,
    )?;
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

pub fn upsert(pool: &DbPool, item: &IncomingListing) -> Result<bool, rusqlite::Error> {
    let conn = pool.get().expect("db pool");
    let id = listing_id(&item.source_url);
    let now = chrono::Utc::now().to_rfc3339();
    let changed = conn.execute(
        r#"
        INSERT INTO listings (
            id, source, source_url, title, company, location, county,
            listing_type, description, published_at, scraped_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
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
        params![
            id,
            item.source,
            item.source_url,
            item.title,
            item.company,
            item.location,
            item.county,
            item.listing_type.as_str(),
            item.description,
            item.published_at,
            now,
        ],
    )?;
    Ok(changed > 0)
}

fn map_listing(row: &rusqlite::Row<'_>) -> rusqlite::Result<Listing> {
    Ok(Listing {
        id: row.get(0)?,
        source: row.get(1)?,
        source_url: row.get(2)?,
        title: row.get(3)?,
        company: row.get(4)?,
        location: row.get(5)?,
        county: row.get(6)?,
        listing_type: row.get(7)?,
        description: row.get(8)?,
        published_at: row.get(9)?,
        scraped_at: row.get(10)?,
    })
}

const COLS: &str = "id, source, source_url, title, company, location, county, listing_type, description, published_at, scraped_at";

pub fn get_by_id(pool: &DbPool, id: &str) -> Result<Option<Listing>, rusqlite::Error> {
    let conn = pool.get().expect("db pool");
    conn.query_row(
        &format!("SELECT {COLS} FROM listings WHERE id = ?1"),
        params![id],
        map_listing,
    )
    .optional()
}

pub fn search(pool: &DbPool, q: &ListQuery) -> Result<(i64, Vec<Listing>), rusqlite::Error> {
    let conn = pool.get().expect("db pool");
    let page = q.page.unwrap_or(1).max(1);
    let limit = q.limit.unwrap_or(24).clamp(1, 100);
    let offset = (page - 1) * limit;

    let mut where_sql = String::from("WHERE 1=1");
    let mut vals: Vec<String> = Vec::new();

    if let Some(t) = q.listing_type.as_deref().and_then(crate::models::ListingType::parse) {
        where_sql.push_str(" AND listing_type = ?");
        vals.push(t.as_str().to_string());
    }
    if let Some(county) = q.county.as_deref().filter(|s| !s.trim().is_empty()) {
        where_sql.push_str(" AND (county LIKE ? OR location LIKE ?)");
        vals.push(format!("%{county}%"));
        vals.push(format!("%{county}%"));
    }
    if let Some(source) = q.source.as_deref().filter(|s| !s.trim().is_empty()) {
        where_sql.push_str(" AND source = ?");
        vals.push(source.to_string());
    }
    if let Some(query) = q.q.as_deref().filter(|s| !s.trim().is_empty()) {
        where_sql.push_str(" AND (title LIKE ? OR company LIKE ? OR description LIKE ?)");
        let like = format!("%{query}%");
        vals.push(like.clone());
        vals.push(like.clone());
        vals.push(like);
    }

    let count_sql = format!("SELECT COUNT(*) FROM listings {where_sql}");
    let mut count_stmt = conn.prepare(&count_sql)?;
    let count_params: Vec<&dyn rusqlite::ToSql> = vals.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
    let total: i64 = count_stmt.query_row(count_params.as_slice(), |row| row.get(0))?;

    let list_sql = format!(
        "SELECT {COLS} FROM listings {where_sql} ORDER BY COALESCE(published_at, scraped_at) DESC LIMIT ? OFFSET ?"
    );
    let mut list_stmt = conn.prepare(&list_sql)?;
    let mut all_params: Vec<&dyn rusqlite::ToSql> = vals.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
    let limit_i = limit as i64;
    let offset_i = offset as i64;
    all_params.push(&limit_i);
    all_params.push(&offset_i);
    let items = list_stmt
        .query_map(all_params.as_slice(), map_listing)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok((total, unique_listings(items, limit as usize)))
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

pub fn latest_by_type(pool: &DbPool, listing_type: &str, limit: i64) -> Result<Vec<Listing>, rusqlite::Error> {
    let conn = pool.get().expect("db pool");
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM listings WHERE listing_type = ?1 ORDER BY COALESCE(published_at, scraped_at) DESC LIMIT ?2"
    ))?;
    let rows = stmt
        .query_map(params![listing_type, limit * 3], map_listing)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(unique_listings(rows, limit as usize))
}

pub fn latest_matching(pool: &DbPool, like: &str, limit: i64) -> Result<Vec<Listing>, rusqlite::Error> {
    let conn = pool.get().expect("db pool");
    let pattern = format!("%{like}%");
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM listings WHERE location LIKE ?1 OR county LIKE ?1 ORDER BY COALESCE(published_at, scraped_at) DESC LIMIT ?2"
    ))?;
    let rows = stmt
        .query_map(params![pattern, limit * 3], map_listing)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(unique_listings(rows, limit as usize))
}

pub fn featured(pool: &DbPool) -> Result<Option<Listing>, rusqlite::Error> {
    let conn = pool.get().expect("db pool");
    for listing_type in ["internship", "attachment"] {
        let found = conn
            .query_row(
                &format!(
                    "SELECT {COLS} FROM listings WHERE listing_type = ?1
                     ORDER BY COALESCE(published_at, scraped_at) DESC LIMIT 1"
                ),
                params![listing_type],
                map_listing,
            )
            .optional()?;
        if found.is_some() {
            return Ok(found);
        }
    }
    conn.query_row(
        &format!(
            "SELECT {COLS} FROM listings ORDER BY COALESCE(published_at, scraped_at) DESC LIMIT 1"
        ),
        [],
        map_listing,
    )
    .optional()
}

pub fn count_type(pool: &DbPool, listing_type: &str) -> Result<i64, rusqlite::Error> {
    let conn = pool.get().expect("db pool");
    conn.query_row(
        "SELECT COUNT(*) FROM listings WHERE listing_type = ?1",
        params![listing_type],
        |row| row.get(0),
    )
}

pub fn count_all(pool: &DbPool) -> Result<i64, rusqlite::Error> {
    let conn = pool.get().expect("db pool");
    conn.query_row("SELECT COUNT(*) FROM listings", [], |row| row.get(0))
}

pub fn refresh_types(pool: &DbPool) -> Result<usize, rusqlite::Error> {
    let conn = pool.get().expect("db pool");
    let mut stmt = conn.prepare("SELECT id, title, description FROM listings")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let mut touched = 0;
    for (id, title, description) in rows {
        if !is_usable_title(&title) {
            conn.execute("DELETE FROM listings WHERE id = ?1", params![id])?;
            touched += 1;
            continue;
        }
        let listing_type = classify(&title, &description);
        conn.execute(
            "UPDATE listings SET listing_type = ?1 WHERE id = ?2",
            params![listing_type.as_str(), id],
        )?;
        touched += 1;
    }
    Ok(touched)
}
