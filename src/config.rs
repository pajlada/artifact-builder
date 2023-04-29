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
pub struct EnvironmentVariable {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DefaultBuild {
    pub cmake_args: Vec<String>,
    pub package_envs: Vec<EnvironmentVariable>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Build {
    pub pre_cmake_commands: Option<Vec<String>>,
    pub cmake_args: Vec<String>,
    pub pre_package_commands: Option<Vec<String>>,
    pub package_envs: Vec<EnvironmentVariable>,
    pub pre_dmg_commands: Option<Vec<String>>,
    pub build_dir: String,
    pub asset_name: String,
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
pub struct BuildConfig {
    pub repo_dir: String,
    pub dmg_output_path: String,

    pub default_config: DefaultBuild,

    pub configs: Vec<Build>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub web: WebConfig,

    pub github: GithubConfig,

    pub build: BuildConfig,
}

pub fn read(path: &str) -> Result<Config> {
    let default_config = r#"
[web]
base_url = "/"

[build]
default_config = { cmake_args = [], package_envs = [] }
configs = []

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

    if config.build.configs.is_empty() {
        return Err(anyhow::anyhow!("Must include at least one build config"));
    }

    Ok(config)
}
