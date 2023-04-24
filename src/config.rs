use anyhow::{Context, Result};
use serde::Deserialize;

use figment::{
    providers::{Format, Toml},
    Figment,
};

#[derive(Debug, Deserialize)]
pub struct GithubConfig {
    pub token: String,

    pub verify_signature: bool,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub github: GithubConfig,
}

pub fn read(path: &str) -> Result<Config> {
    let default_config = r#"
[github]
verify_signature = true
"#;

    let config: Config = Figment::new()
        .merge(Toml::string(default_config))
        .merge(Toml::file(path))
        .extract()
        .context(format!("Error loading config from {path}"))?;

    Ok(config)
}
