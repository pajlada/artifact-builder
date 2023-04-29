use crate::config;

const USER_AGENT: &str = "chatterino-macos-artifact-builder 0.1.0";

pub fn build(cfg: &config::GithubConfig) -> anyhow::Result<reqwest::Client> {
    let authorization_value: String = format!("Bearer {}", cfg.token);

    let mut default_headers = reqwest::header::HeaderMap::new();
    default_headers.insert(
        "User-Agent",
        reqwest::header::HeaderValue::from_static(USER_AGENT),
    );
    default_headers.insert(
        "Accept",
        reqwest::header::HeaderValue::from_static("application/vnd.github+json"),
    );
    default_headers.insert(
        "Authorization",
        reqwest::header::HeaderValue::from_str(authorization_value.as_str()).unwrap(),
    );

    let client = reqwest::Client::builder()
        .default_headers(default_headers)
        .build()?;

    Ok(client)
}
