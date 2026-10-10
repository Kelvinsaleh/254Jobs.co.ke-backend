use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ListingType {
    Job,
    Internship,
    Attachment,
}

impl ListingType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ListingType::Job => "job",
            ListingType::Internship => "internship",
            ListingType::Attachment => "attachment",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "job" | "jobs" => Some(Self::Job),
            "internship" | "internships" | "intern" => Some(Self::Internship),
            "attachment" | "attachments" => Some(Self::Attachment),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Listing {
    pub id: String,
    pub source: String,
    pub source_url: String,
    pub title: String,
    pub company: Option<String>,
    pub location: String,
    pub county: Option<String>,
    pub listing_type: String,
    pub description: String,
    pub published_at: Option<String>,
    pub scraped_at: String,
}

#[derive(Debug, Clone)]
pub struct IncomingListing {
    pub source: String,
    pub source_url: String,
    pub title: String,
    pub company: Option<String>,
    pub location: String,
    pub county: Option<String>,
    pub listing_type: ListingType,
    pub description: String,
    pub published_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    #[serde(rename = "type")]
    pub listing_type: Option<String>,
    pub county: Option<String>,
    pub source: Option<String>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ListResponse {
    pub total: i64,
    pub page: u32,
    pub limit: u32,
    pub items: Vec<Listing>,
}

#[derive(Debug, Serialize)]
pub struct HomeResponse {
    pub featured: Option<Listing>,
    pub internships: Vec<Listing>,
    pub attachments: Vec<Listing>,
    pub jobs: Vec<Listing>,
    pub nairobi: Vec<Listing>,
    pub remote: Vec<Listing>,
    pub stats: Stats,
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub jobs: i64,
    pub internships: i64,
    pub attachments: i64,
    pub total: i64,
}

#[derive(Debug, Serialize)]
pub struct ScrapeReport {
    pub source: String,
    pub fetched: usize,
    pub upserted: usize,
    pub error: Option<String>,
}
