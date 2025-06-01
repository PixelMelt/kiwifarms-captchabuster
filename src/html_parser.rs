use scraper::{Html, Selector};
use regex::Regex;
use once_cell::sync::Lazy;

static SCRIPT_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse("script").expect("Failed to parse script selector"));
static CHALLENGE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"window\.sssg_challenge\s*\(\s*['"]([^'"]+)['"]\s*,\s*(\d+)\s*,\s*(\d+)\s*\)"#)
        .expect("Failed to compile challenge regex")
});

#[derive(Debug)]
pub enum ParseError {
    SelectorError(String),
    ChallengeScriptNotFound,
    RegexError(regex::Error),
    ParameterNotFound(String),
    InvalidParameterValue(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::SelectorError(s) => write!(f, "HTML selector error: {}", s),
            ParseError::ChallengeScriptNotFound => write!(f, "SSSG challenge script not found in HTML"),
            ParseError::RegexError(e) => write!(f, "Regex error: {}", e),
            ParseError::ParameterNotFound(param) => write!(f, "Challenge parameter '{}' not found", param),
            ParseError::InvalidParameterValue(param) => write!(f, "Invalid value for challenge parameter '{}'", param),
        }
    }
}

impl std::error::Error for ParseError {}

/// Extracts the `salt` and `difficulty` from the HTML content.
/// It looks for a script tag containing `window.sssg_challenge(...)`.
pub fn extract_challenge_params(html_content: &str) -> Result<(String, u32), ParseError> {
    let document = Html::parse_document(html_content);
    // Use the lazily initialized selector and regex
    // let script_selector = Selector::parse("script").map_err(|e| ParseError::SelectorError(e.to_string()))?; // Replaced by SCRIPT_SELECTOR

    // Regex to find the sssg_challenge call and capture its arguments
    // Example: window.sssg_challenge("salt_value", difficulty_value, timeout_value);
    // let re = Regex::new(r#"window\.sssg_challenge\s*\(\s*['"]([^'"]+)['"]\s*,\s*(\d+)\s*,\s*(\d+)\s*\)"#)
    //     .map_err(ParseError::RegexError)?; // Replaced by CHALLENGE_RE

    for script_element in document.select(&SCRIPT_SELECTOR) {
        if let Some(script_text) = script_element.text().next() {
            if let Some(captures) = CHALLENGE_RE.captures(script_text) {
                let salt = captures.get(1)
                    .ok_or_else(|| ParseError::ParameterNotFound("salt".to_string()))?
                    .as_str()
                    .to_string();
                let difficulty_str = captures.get(2)
                    .ok_or_else(|| ParseError::ParameterNotFound("difficulty".to_string()))?
                    .as_str();
                let difficulty = difficulty_str.parse::<u32>()
                    .map_err(|_| ParseError::InvalidParameterValue(format!("difficulty: {}", difficulty_str)))?;
                
                // The third parameter (timeout) is captured but not used in this function's return.
                // let _timeout = captures.get(3)
                //     .ok_or_else(|| ParseError::ParameterNotFound("timeout".to_string()))?
                //     .as_str();

                return Ok((salt, difficulty));
            }
        }
    }

    Err(ParseError::ChallengeScriptNotFound)
}