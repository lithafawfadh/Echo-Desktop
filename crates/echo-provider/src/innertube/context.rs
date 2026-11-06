use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, CONTENT_TYPE, ORIGIN, REFERER, USER_AGENT};
use serde_json::{json, Value};
use std::time::Duration;

pub const INNERTUBE_CLIENT_NAME_ID: &str = "67";
pub const INNERTUBE_CLIENT_NAME: &str = "WEB_REMIX";
pub const INNERTUBE_CLIENT_VERSION: &str = "1.20240401.01.00";
pub const INNERTUBE_BASE_URL: &str = "https://music.youtube.com/youtubei/v1";

/// Constructs the standardized YouTube Music `WEB_REMIX` context JSON payload.
pub fn build_client_context() -> Value {
    json!({
        "client": {
            "clientName": INNERTUBE_CLIENT_NAME,
            "clientVersion": INNERTUBE_CLIENT_VERSION,
            "hl": "en",
            "gl": "US",
            "experimentIds": [],
            "utcOffsetMinutes": 0
        },
        "user": {
            "lockedSafetyMode": false
        },
        "request": {
            "useSsl": true
        }
    })
}

/// Creates a pre-configured `reqwest::Client` matching the official YouTube Music web client.
pub fn create_innertube_http_client() -> Result<reqwest::Client, reqwest::Error> {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
        ),
    );
    headers.insert(ORIGIN, HeaderValue::from_static("https://music.youtube.com"));
    headers.insert(REFERER, HeaderValue::from_static("https://music.youtube.com/"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));
    headers.insert("X-YouTube-Client-Name", HeaderValue::from_static(INNERTUBE_CLIENT_NAME_ID));
    headers.insert("X-YouTube-Client-Version", HeaderValue::from_static(INNERTUBE_CLIENT_VERSION));
    headers.insert("Sec-Fetch-Mode", HeaderValue::from_static("cors"));
    headers.insert("Sec-Fetch-Site", HeaderValue::from_static("same-origin"));

    reqwest::Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
}
