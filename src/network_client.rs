use reqwest::{Client, Error as ReqwestError, StatusCode};
use std::time::Instant;
use serde::{Deserialize};
use log::{debug, info, warn};
use serde_json; // Added for from_str
use url::Url;

#[derive(Debug, Deserialize)]
struct AnswerResponse {
    auth: String,
}

#[derive(Debug, Deserialize)]
struct CheckResponse {
    auth: String,
}

#[derive(Debug)]
pub enum NetworkError {
    Reqwest(ReqwestError),
    ApiError { status: StatusCode, message: String },
    UrlParseError(url::ParseError),
    MissingAuthToken(String),
    SerdeJsonError(serde_json::Error),
}

impl From<ReqwestError> for NetworkError {
    fn from(err: ReqwestError) -> NetworkError {
        NetworkError::Reqwest(err)
    }
}

impl From<url::ParseError> for NetworkError {
    fn from(err: url::ParseError) -> NetworkError {
        NetworkError::UrlParseError(err)
    }
}

impl From<serde_json::Error> for NetworkError {
    fn from(err: serde_json::Error) -> NetworkError {
        NetworkError::SerdeJsonError(err)
    }
}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkError::Reqwest(e) => write!(f, "HTTP request error: {}", e),
            NetworkError::ApiError { status, message } => write!(f, "API error ({}): {}", status, message),
            NetworkError::UrlParseError(e) => write!(f, "URL parsing error: {}", e),
            NetworkError::MissingAuthToken(context) => write!(f, "Missing auth token in API response ({})", context),
            NetworkError::SerdeJsonError(e) => write!(f, "JSON deserialization error: {}", e),
        }
    }
}

impl std::error::Error for NetworkError {}

/// Fetches the initial HTML content from the given URL.
pub async fn fetch_initial_page_html(client: &Client, url_str: &str) -> Result<String, NetworkError> {
    let start_time = Instant::now();
    let response_result = client.get(url_str).send().await;
    let duration = start_time.elapsed();
    info!("[TIMING] fetch_initial_page_html for {} took {:.2?}", url_str, duration);

    let response = response_result?;
    if !response.status().is_success() {
        return Err(NetworkError::ApiError {
            status: response.status(),
            message: format!("Failed to fetch initial page: {}", url_str),
        });
    }
    Ok(response.text().await?)
}

/// Submits the Proof-of-Work solution to the /answer endpoint.
/// Returns the temporary authentication token.
pub async fn submit_pow_answer(client: &Client, base_url: &Url, salt: &str, successful_attempt_str: &str) -> Result<String, NetworkError> {
    let origin = base_url.origin().unicode_serialization();
    let answer_url_str = format!("{}/.sssg/api/answer", origin);
    let answer_url = Url::parse(&answer_url_str)?;
    
    let params = [("a", salt), ("b", successful_attempt_str)];
    debug!("[API] Sending POST to /answer URL: {}", answer_url);
    debug!("[API] /answer form params: {:?}", params);

    let start_time = Instant::now();
    let response_result = client.post(answer_url.clone())
        .form(&params)
        .send()
        .await;
    let duration = start_time.elapsed();
    info!("[TIMING] submit_pow_answer to {} took {:.2?}", answer_url, duration);
    
    let response = response_result?;

    if !response.status().is_success() {
        let status_code = response.status();
        let error_text = match response.text().await {
            Ok(text) => text,
            Err(e) => format!("Failed to read error body (detail: {}). Original status: {}", e, status_code),
        };
        return Err(NetworkError::ApiError {
            status: status_code,
            message: format!("Failed to submit to /answer. Server response: {}", error_text),
        });
    }

    let response_text = response.text().await?;
    debug!("[API] /answer response body: {}", response_text);
    let answer_json: AnswerResponse = serde_json::from_str(&response_text)
        .map_err(NetworkError::from)?;
    Ok(answer_json.auth)
}

/// Submits the temporary authentication token to the /check endpoint.
/// Returns the final sssg_clearance token.
pub async fn submit_final_check(client: &Client, base_url: &Url, temp_auth_token: &str) -> Result<String, NetworkError> {
    let origin = base_url.origin().unicode_serialization();
    let check_url_str = format!("{}/.sssg/api/check", origin);
    let check_url = Url::parse(&check_url_str)?;

    let params = [("f", temp_auth_token)];
    debug!("[API] Sending POST to /check URL: {}", check_url);
    debug!("[API] /check form params: {:?}", params);

    let start_time = Instant::now();
    let response_result = client.post(check_url.clone())
        .form(&params)
        .send()
        .await;
    let duration = start_time.elapsed();
    info!("[TIMING] submit_final_check to {} took {:.2?}", check_url, duration);

    let response = response_result?;

    if !response.status().is_success() {
        let status_code = response.status();
        let error_text = match response.text().await {
            Ok(text) => text,
            Err(e) => format!("Failed to read error body (detail: {}). Original status: {}", e, status_code),
        };
        return Err(NetworkError::ApiError {
            status: status_code,
            message: format!("Failed to submit to /check. Server response: {}", error_text),
        });
    }

    let response_text = response.text().await?;
    debug!("[API] /check response body: {}", response_text);
    let check_json: CheckResponse = serde_json::from_str(&response_text)
        .map_err(NetworkError::from)?;
    Ok(check_json.auth)
}

/// Fetches HTML content from the given URL using the client (which should have cookies set).
pub async fn fetch_page_html_with_cookies(client: &Client, url_str: &str) -> Result<String, NetworkError> {
    let start_time = Instant::now();
    let response_result = client.get(url_str).send().await;
    let duration = start_time.elapsed();
    info!("[TIMING] fetch_page_html_with_cookies for {} took {:.2?}", url_str, duration);

    let response = response_result?;
    if !response.status().is_success() {
        return Err(NetworkError::ApiError {
            status: response.status(),
            message: format!("Failed to fetch page HTML from {}: {}", url_str, response.status()),
        });
    }
    Ok(response.text().await?)
}