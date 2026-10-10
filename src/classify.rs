use crate::models::ListingType;
use regex::Regex;
use std::sync::OnceLock;

const COUNTIES: &[&str] = &[
    "Nairobi",
    "Mombasa",
    "Kisumu",
    "Nakuru",
    "Kiambu",
    "Machakos",
    "Kajiado",
    "Uasin Gishu",
    "Eldoret",
    "Nyeri",
    "Meru",
    "Kisii",
    "Kakamega",
    "Bungoma",
    "Kericho",
    "Kilifi",
    "Kwale",
    "Garissa",
    "Turkana",
    "Kitale",
    "Thika",
    "Naivasha",
    "Malindi",
    "Embu",
    "Nyandarua",
    "Bomet",
    "Narok",
    "Migori",
    "Homa Bay",
    "Siaya",
    "Busia",
    "Vihiga",
    "Nandi",
    "Trans Nzoia",
    "Laikipia",
    "Murang'a",
    "Muranga",
    "Makueni",
    "Kitui",
    "Taita Taveta",
    "Lamu",
    "Isiolo",
    "Marsabit",
    "Mandera",
    "Wajir",
    "Samburu",
    "West Pokot",
    "Elgeyo Marakwet",
    "Baringo",
];

fn intern_word() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\bintern(?:s|ship)?\b").expect("regex"))
}

fn title_at_company() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^(.+?)\s+at\s+(.+)$").expect("regex"))
}

pub fn classify(title: &str, _description: &str) -> ListingType {
    let title_l = title.to_ascii_lowercase();

    if title_l.contains("intern trainer") || title_l.contains("internship coordinator") {
        return ListingType::Job;
    }

    if title_l.contains("industrial attachment")
        || title_l.contains("student attachment")
        || title_l.contains("attachee")
        || title_l.contains("attaché")
        || title_l.contains(" on attachment")
        || title_l.contains("attachment opportunity")
        || title_l.contains("attachment programme")
        || title_l.contains("attachment program")
        || title_l.contains("attachment -")
        || title_l.starts_with("attachment ")
    {
        return ListingType::Attachment;
    }

    // "attache" matches attachee-style titles, not "attachment" in email instructions.
    if intern_word().is_match(&title_l)
        || title_l.contains("graduate trainee")
        || title_l.contains("graduate program")
        || title_l.contains("graduate programme")
        || title_l.contains("pupillage")
        || title_l.contains("articleship")
    {
        return ListingType::Internship;
    }

    ListingType::Job
}

pub fn is_usable_title(title: &str) -> bool {
    let t = collapse_ws(title);
    if t.chars().count() < 8 {
        return false;
    }
    let lower = t.to_ascii_lowercase();
    if lower.contains("uk market") {
        return false;
    }
    let exact_junk = [
        "active adverts",
        "vacancies",
        "vacant roles",
        "latest jobs",
        "jobs in kenya",
        "internship roles",
    ];
    if exact_junk.iter().any(|j| lower == *j) {
        return false;
    }
    if lower.starts_with("submit cvs")
        && !lower.contains("intern")
        && !lower.contains("attachment")
    {
        return false;
    }
    true
}

pub fn decode_text(input: &str) -> String {
    collapse_ws(
        &input
            .replace("&amp;", "&")
            .replace("&nbsp;", " ")
            .replace("&ndash;", "–")
            .replace("&mdash;", "—")
            .replace("&#8211;", "–")
            .replace("&#8212;", "—")
            .replace("â€“", "–")
            .replace("Ã¢Â€Â“", "–"),
    )
}

pub fn split_title_company(title: &str) -> (String, Option<String>) {
    if let Some(caps) = title_at_company().captures(title.trim()) {
        let role = caps.get(1).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
        let company = caps.get(2).map(|m| m.as_str().trim().to_string());
        if !role.is_empty() {
            return (role, company);
        }
    }
    (title.trim().to_string(), None)
}

pub fn extract_location(title: &str, description: &str, fallback: &str) -> (String, Option<String>) {
    let blob = format!("{title}\n{description}\n{fallback}");
    let lower = blob.to_ascii_lowercase();
    if lower.contains("remote") || lower.contains("work from home") {
        return ("Remote".into(), Some("Remote".into()));
    }
    for county in COUNTIES {
        if lower.contains(&county.to_ascii_lowercase()) {
            let label = if *county == "Eldoret" {
                "Uasin Gishu"
            } else if *county == "Thika" {
                "Kiambu"
            } else {
                *county
            };
            return (county.to_string(), Some(label.to_string()));
        }
    }
    if lower.contains("kenya") {
        return ("Kenya".into(), None);
    }
    let fb = fallback.trim();
    if fb.is_empty() {
        ("Kenya".into(), None)
    } else {
        (fb.to_string(), None)
    }
}

pub fn strip_html(input: &str) -> String {
    let frag = scraper::Html::parse_fragment(input);
    let text = frag.root_element().text().collect::<Vec<_>>().join(" ");
    decode_text(&text)
}

pub fn collapse_ws(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn truncate(input: &str, max: usize) -> String {
    let chars: Vec<char> = input.chars().collect();
    if chars.len() <= max {
        input.to_string()
    } else {
        format!("{}…", chars[..max].iter().collect::<String>())
    }
}

pub fn looks_like_kenya(title: &str, description: &str, location: &str, url: &str) -> bool {
    let blob = format!("{title} {description} {location} {url}").to_ascii_lowercase();
    blob.contains("kenya")
        || blob.contains("nairobi")
        || blob.contains("mombasa")
        || blob.contains(".co.ke")
        || blob.contains("jobwebkenya")
        || blob.contains("myjobmag.co.ke")
}
