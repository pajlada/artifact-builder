mod build;
mod config;
mod git;
mod github;
mod web;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // TODO: Add the ability to specify a custom config path
    let cfg = config::read("config.toml")?;

    let github_client = github::client::build(&cfg.github)?;

    web::start_server(cfg, github_client).await?;

    Ok(())
}
