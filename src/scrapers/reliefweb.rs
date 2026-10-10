use crate::classify::{classify, extract_location, is_usable_title, truncate};
use crate::models::IncomingListing;

const URL: &str = "https://api.reliefweb.int/v2/jobs?appname=254jobs-kenya&profile=list&limit=40&query[value]=Kenya&query[fields][]=country";

pub async fn fetch() -> Result<Vec<IncomingListing>, String> {
    let json = super::http::get_json(URL).await?;
    let Some(data) = json.get("data").and_then(|v| v.as_array()) else {
        return Err("reliefweb payload missing data".into());
    };
    let mut items = Vec::new();
    for entry in data {
        let fields = entry.get("fields").cloned().unwrap_or(serde_json::Value::Null);
        let title = fields
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let url = fields
            .get("url")
            .and_then(|v| v.as_str())
            .or_else(|| fields.get("url_alias").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();
        if title.is_empty() || url.is_empty() || !is_usable_title(&title) {
            continue;
        }
        let company = fields
            .get("source")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|s| s.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let body = fields
            .get("body")
            .or_else(|| fields.get("body-html"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let description = truncate(&crate::classify::strip_html(body), 1800);
        let listing_type = classify(&title, &description);
        let (location, county) = extract_location(&title, &description, "Kenya");
        let published = fields
            .get("date")
            .and_then(|d| d.get("created"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        items.push(IncomingListing {
            source: "reliefweb".into(),
            source_url: url,
            title,
            company,
            location,
            county,
            listing_type,
            description,
            published_at: published,
        });
    }
    if items.is_empty() {
        return Err("reliefweb returned no Kenya listings".into());
    }
    Ok(items)
}
