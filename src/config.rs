use anyhow::{Context, Result};
use serde::Deserialize;

use figment::{
    providers::{Format, Toml},
    Figment,
};

#[derive(Debug, Deserialize, Clone)]
pub struct BranchAndRelease {
    pub name: String,
    pub release_id: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GithubConfig {
    pub token: String,

    pub verify_signature: bool,

    pub secret: String,

    pub repo_owner: String,
    pub repo_name: String,

    pub branches: Vec<BranchAndRelease>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WebConfig {
    pub base_url: String,

    pub bind: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub web: WebConfig,

    pub github: GithubConfig,
}

pub fn read(path: &str) -> Result<Config> {
    let default_config = r#"
[web]
base_url = "/"

[github]
verify_signature = true
"#;

    let config: Config = Figment::new()
        .merge(Toml::string(default_config))
        .merge(Toml::file(path))
        .extract()
        .context(format!("Error loading config from {path}"))?;

    if config.web.bind.is_empty() {
        return Err(anyhow::anyhow!("Must include at least one bind interface"));
    }

    Ok(config)
}
