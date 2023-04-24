use anyhow::{Context, Result};
use serde::Deserialize;

use figment::{
    providers::{Format, Toml},
    Figment,
};

#[derive(Debug, Deserialize)]
pub struct GithubConfig {
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub github: GithubConfig,
}

pub fn read(path: &str) -> Result<Config> {
    let config: Config = Figment::new()
        .merge(Toml::file(path))
        .extract()
        .context(format!("Error loading config from {path}"))?;

    Ok(config)
}
