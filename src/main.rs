mod network_client;
mod html_parser;
mod pow_solver;
mod utils;

use clap::Parser;
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, CACHE_CONTROL, CONNECTION, CONTENT_TYPE, ORIGIN, PRAGMA, REFERER, USER_AGENT, HeaderName};
use url::Url;
use once_cell::sync::Lazy;
use log::{info, debug, error, warn};

static BASE_HEADERS: Lazy<HeaderMap> = Lazy::new(|| {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.5"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(CONNECTION, HeaderValue::from_static("keep-alive"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/x-www-form-urlencoded"));
    headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
    headers.insert(HeaderName::from_static("sec-fetch-dest"), HeaderValue::from_static("empty"));
    headers.insert(HeaderName::from_static("sec-fetch-mode"), HeaderValue::from_static("cors"));
    headers.insert(HeaderName::from_static("sec-fetch-site"), HeaderValue::from_static("same-origin"));
    headers.insert(HeaderName::from_static("sec-gpc"), HeaderValue::from_static("1"));
    headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36"));
    headers.insert(HeaderName::from_static("sec-ch-ua"), HeaderValue::from_static("\"Brave\";v=\"135\", \"Not-A.Brand\";v=\"8\", \"Chromium\";v=\"135\""));
    headers.insert(HeaderName::from_static("sec-ch-ua-mobile"), HeaderValue::from_static("?0"));
    headers.insert(HeaderName::from_static("sec-ch-ua-platform"), HeaderValue::from_static("\"Linux\""));
    headers
});

// Custom Application Error Type
#[derive(Debug)]
enum AppError {
    Network(network_client::NetworkError),
    Parse(html_parser::ParseError),
    Io(std::io::Error),
    UrlParse(url::ParseError),
    Boxed(Box<dyn std::error::Error>), // For other generic errors
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Network(err) => write!(f, "Network error: {}", err),
            AppError::Parse(err) => write!(f, "Parsing error: {}", err),
            AppError::Io(err) => write!(f, "IO error: {}", err),
            AppError::UrlParse(err) => write!(f, "URL parsing error: {}", err),
            AppError::Boxed(err) => write!(f, "Error: {}", err),
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AppError::Network(err) => Some(err),
            AppError::Parse(err) => Some(err),
            AppError::Io(err) => Some(err),
            AppError::UrlParse(err) => Some(err),
            AppError::Boxed(err) => Some(err.as_ref()),
        }
    }
}

impl From<network_client::NetworkError> for AppError {
    fn from(err: network_client::NetworkError) -> Self {
        AppError::Network(err)
    }
}

impl From<html_parser::ParseError> for AppError {
    fn from(err: html_parser::ParseError) -> Self {
        AppError::Parse(err)
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err)
    }
}

impl From<url::ParseError> for AppError {
    fn from(err: url::ParseError) -> Self {
        AppError::UrlParse(err)
    }
}

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        AppError::Network(network_client::NetworkError::from(err))
    }
}
impl From<Box<dyn std::error::Error>> for AppError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        AppError::Boxed(err)
    }
}


#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(value_parser)]
    url: String,

    #[clap(long)]
    html: bool,

    #[clap(long)] // If present, perform the /check call. Default is to skip.
    check: bool,
}

#[tokio::main]
async fn main() -> Result<(), AppError> {
    let args = Args::parse();
    env_logger::init(); // Initialize logger

    let suppress_logging = args.html; // We'll keep this for now to control HTML output, but phase out for general logging

    if !suppress_logging { // This specific one might stay if it's considered direct user output not a "log"
        println!("Target URL: {}", args.url);
    }
    info!("Target URL: {}", args.url);


    let base_url = Url::parse(&args.url)?; // url::ParseError converted via From trait
    let origin_url = base_url.origin().unicode_serialization();

    let mut headers = BASE_HEADERS.clone(); // Clone the lazily initialized base headers
    // Add dynamic headers
    if let Ok(origin_val) = HeaderValue::from_str(&origin_url) {
        headers.insert(ORIGIN, origin_val);
    }
    if let Ok(referer_val) = HeaderValue::from_str(&args.url) { // Use the full URL as referer
        headers.insert(REFERER, referer_val);
    }

    let client = Client::builder()
        .default_headers(headers)
        .cookie_provider(std::sync::Arc::new(reqwest::cookie::Jar::default()))
        .build()?; // reqwest::Error converted via From trait

    // let origin = base_url.origin().unicode_serialization(); // Now used for ORIGIN header

    // 1. Fetch initial page and extract challenge parameters
    info!("Fetching initial page...");
    let html_content = network_client::fetch_initial_page_html(&client, &args.url).await?;
    info!("Page fetched. Extracting challenge parameters...");
    let (salt, difficulty) = html_parser::extract_challenge_params(&html_content)?;
    info!("Salt: {}, Difficulty: {}", salt, difficulty);

    // 2. Solve PoW
    let num_threads = num_cpus::get();
    let initial_attempt_seed = utils::generate_initial_attempt_nonce_seed();
    info!("Starting PoW with difficulty {} on {} threads (initial seed: {})...", difficulty, num_threads, initial_attempt_seed);

    if let Some((successful_attempt, solution_hash_hex)) = pow_solver::solve_challenge(&salt, difficulty, initial_attempt_seed, num_threads) {
        info!("Solution found!");
        info!("\tAttempt: {}", successful_attempt);
        info!("\tHash:    {}", solution_hash_hex);

        // 3. Submit solution
        info!("Submitting solution to /answer...");
        let temp_auth_token = network_client::submit_pow_answer(&client, &base_url, &salt, &successful_attempt).await?;
        info!("Auth token from /answer response: {}", temp_auth_token);

        let final_clearance_token_to_report: String;

        if args.check {
            info!("--check flag is set. Submitting token from /answer to /check endpoint...");
            let token_from_check = network_client::submit_final_check(&client, &base_url, &temp_auth_token).await?;
            final_clearance_token_to_report = token_from_check;
            if !suppress_logging { // This is direct output to user
                println!("\nSuccessfully obtained sssg_clearance token (from /check): {}", final_clearance_token_to_report);
            } else {
                info!("Successfully obtained sssg_clearance token (from /check): {}", final_clearance_token_to_report);
            }
        } else {
            info!("Skipping /check endpoint by default. Using cookie from /answer response.");
            // The cookie jar in `client` is automatically updated by reqwest
            // The temp_auth_token is the value from the /answer JSON body.
            final_clearance_token_to_report = temp_auth_token;
            if !suppress_logging { // This is direct output to user
                println!("\nSSSG Clearance obtained (from /answer): {}", final_clearance_token_to_report);
            } else {
                info!("SSSG Clearance obtained (from /answer): {}", final_clearance_token_to_report);
            }
        }

        if args.html {
            info!("\nFetching final page HTML with current sssg_clearance cookie...");
            // The client now has the sssg_clearance cookie in its jar
            let final_html_content = network_client::fetch_page_html_with_cookies(&client, &args.url).await?;
            // This println call is for the actual HTML output, so it is not suppressed by RUST_LOG.
            println!("{}", final_html_content);
        }

    } else {
        warn!("No solution found for the PoW challenge.");
        // Consider returning an error here if no solution is an actual failure condition
        // For now, it just prints and exits successfully.
    }

    Ok(())
}
