use crate::classify::{
    classify, collapse_ws, decode_text, extract_location, is_usable_title, looks_like_kenya,
    split_title_company, strip_html, truncate,
};
use crate::models::IncomingListing;
use scraper::{Html, Selector};

const FEEDS: &[&str] = &[
    "https://www.myjobmag.co.ke/jobsxml.xml",
    "https://www.myjobmag.co.ke/jobsxml_by_categories.xml",
];

const PAGES: &[&str] = &[
    "https://www.myjobmag.co.ke/jobs",
    "https://www.myjobmag.co.ke/jobs-by-field/internship",
    "https://www.myjobmag.co.ke/search/jobs?q=internship",
    "https://www.myjobmag.co.ke/search/jobs?q=industrial+attachment",
];

pub async fn fetch() -> Result<Vec<IncomingListing>, String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for feed in FEEDS {
        match super::http::get_text(feed).await {
            Ok(body) => out.extend(parse_rss(&body, &mut seen)),
            Err(err) => tracing::warn!(feed, error = %err, "myjobmag rss skipped"),
        }
    }

    for page in PAGES {
        match super::http::get_text(page).await {
            Ok(body) => out.extend(parse_html(&body, &mut seen)),
            Err(err) => tracing::warn!(page, error = %err, "myjobmag html skipped"),
        }
    }

    if out.is_empty() {
        return Err("myjobmag returned no listings".into());
    }
    Ok(out)
}

fn parse_rss(body: &str, seen: &mut std::collections::HashSet<String>) -> Vec<IncomingListing> {
    let Ok(channel) = body.parse::<rss::Channel>() else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for item in channel.items {
        let url = item.link().unwrap_or("").trim().to_string();
        let raw_title = item.title().unwrap_or("").trim().to_string();
        if url.is_empty() || raw_title.is_empty() || !seen.insert(url.clone()) {
            continue;
        }
        let description = truncate(
            &strip_html(item.description().unwrap_or("")),
            1800,
        );
        if !looks_like_kenya(&raw_title, &description, "Kenya", &url) {
            continue;
        }
        let (title, company) = split_title_company(&decode_text(&raw_title));
        if !is_usable_title(&title) {
            continue;
        }
        let listing_type = classify(&title, &description);
        let (location, county) = extract_location(&title, &description, "Kenya");
        items.push(IncomingListing {
            source: "myjobmag".into(),
            source_url: url,
            title,
            company,
            location,
            county,
            listing_type,
            description,
            published_at: item.pub_date().map(|d| d.to_string()),
        });
    }
    items
}

fn parse_html(body: &str, seen: &mut std::collections::HashSet<String>) -> Vec<IncomingListing> {
    let doc = Html::parse_document(body);
    let Ok(link_sel) = Selector::parse("a") else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for el in doc.select(&link_sel) {
        let href = el.value().attr("href").unwrap_or("");
        let Some(url) = normalize_job_url(href) else {
            continue;
        };
        let raw_title = decode_text(&collapse_ws(&el.text().collect::<String>()));
        if !is_usable_title(&raw_title) || !seen.insert(url.clone()) {
            continue;
        }
        let (title, company) = split_title_company(&raw_title);
        let listing_type = classify(&title, "");
        let (location, county) = extract_location(&title, "", "Kenya");
        items.push(IncomingListing {
            source: "myjobmag".into(),
            source_url: url,
            title,
            company,
            location,
            county,
            listing_type,
            description: raw_title,
            published_at: None,
        });
    }
    items
}

fn normalize_job_url(href: &str) -> Option<String> {
    if href.contains("/job/") || href.contains("/jobs/") {
        if href.starts_with("http") {
            Some(href.to_string())
        } else {
            Some(format!("https://www.myjobmag.co.ke{}", href))
        }
    } else {
        None
    }
}
