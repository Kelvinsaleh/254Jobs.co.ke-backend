use crate::classify::{
    classify, decode_text, extract_location, is_usable_title, split_title_company, strip_html,
    truncate,
};
use crate::models::IncomingListing;

const FEEDS: &[&str] = &[
    "https://jobwebkenya.com/feed/",
    "https://jobwebkenya.com/job-type/internship/feed/",
    "https://jobwebkenya.com/?s=internship&feed=rss2",
    "https://jobwebkenya.com/?s=industrial+attachment&feed=rss2",
];

pub async fn fetch() -> Result<Vec<IncomingListing>, String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for feed in FEEDS {
        match super::http::get_text(feed).await {
            Ok(body) => out.extend(parse_rss(&body, &mut seen)),
            Err(err) => tracing::warn!(feed, error = %err, "jobweb feed skipped"),
        }
    }
    if out.is_empty() {
        return Err("jobweb returned no listings".into());
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
        let html = item
            .content()
            .or(item.description())
            .unwrap_or("")
            .to_string();
        let description = truncate(&strip_html(&html), 1800);
        let (title, company) = split_title_company(&decode_text(&raw_title));
        if !is_usable_title(&title) {
            continue;
        }
        let listing_type = classify(&title, &description);
        let (location, county) = extract_location(&title, &description, "Kenya");
        items.push(IncomingListing {
            source: "jobweb".into(),
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
