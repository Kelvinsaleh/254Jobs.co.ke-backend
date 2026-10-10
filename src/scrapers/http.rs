use reqwest::Client;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::time::sleep;

const USER_AGENT: &str =
    "254jobs/0.1 (Kenya public jobs aggregator; listings only; polite crawl)";

pub fn client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(25))
            .gzip(true)
            .build()
            .expect("http client")
    })
}

pub async fn get_text(url: &str) -> Result<String, String> {
    sleep(Duration::from_millis(700)).await;
    let response = client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("request {url}: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("{url} returned {status}"));
    }
    response
        .text()
        .await
        .map_err(|e| format!("read {url}: {e}"))
}

pub async fn get_json(url: &str) -> Result<serde_json::Value, String> {
    sleep(Duration::from_millis(400)).await;
    let response = client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("request {url}: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("{url} returned {status}"));
    }
    response
        .json()
        .await
        .map_err(|e| format!("json {url}: {e}"))
}
