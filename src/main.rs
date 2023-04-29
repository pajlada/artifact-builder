#[allow(clippy::too_many_arguments)]
mod build;
mod config;
mod git;
mod github;
mod web;

use std::sync::Arc;

use build::pipeline::Pipeline;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // TODO: Add the ability to specify a custom config path
    let cfg = config::read("config.toml")?;

    let github_client = github::client::build(&cfg.github)?;

    let default_build_config = cfg.build.default_config.clone();
    let pipelines: build::Pipelines = cfg
        .github
        .branches
        .iter()
        .map(|branch| {
            let repo_owner = cfg.github.repo_owner.clone();
            let repo_name = cfg.github.repo_name.clone();
            (
                branch.name.clone(),
                cfg.build
                    .configs
                    .iter()
                    .map(|c| {
                        Arc::new(Pipeline::new(
                            github_client.clone(),
                            &cfg.build.repo_dir,
                            &cfg.build.dmg_output_path,
                            repo_owner.clone(),
                            repo_name.clone(),
                            branch,
                            &default_build_config,
                            c.clone(),
                        ))
                    })
                    .collect(),
            )
        })
        .collect();

    web::start_server(cfg, pipelines, github_client).await?;

    Ok(())
}
