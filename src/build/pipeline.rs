use std::{collections::HashMap, path::PathBuf};

use anyhow::Context;

use tracing::log::*;

use crate::github::{self, model::UploadReleaseAssetRoot};

use super::run_command;

pub struct Pipeline {
    github_client: reqwest::Client,

    // Directory where this repo is cloned & built
    // Must not be shared with a second pipeline
    repo_dir: PathBuf,

    // the build directory, where the artifact is built
    // Must not be shared with a second pipeline
    build_dir: PathBuf,

    // Absolute path to where this pipeline's .dmg file will be output
    artifact_path: PathBuf,

    // Name of the asset as it is uploaded into the GitHub release
    asset_name: String,

    repo_url: String,
    // https://github.com/{repo_owner}/{repo_name}
    repo_owner: String,
    // https://github.com/{repo_owner}/{repo_name}
    repo_name: String,

    release_id: u64,
    // todo: BUILD CONFIG
    // todo: ARTIFACT NAME
    cmake_command: String,

    package_envs: HashMap<String, String>,
}

impl Pipeline {
    pub fn new(
        github_client: reqwest::Client,
        repo_dir: &str,
        dmg_output_path: &str,
        repo_owner: String,
        repo_name: String,
        branch: &crate::config::BranchAndRelease,
        default_cfg: &crate::config::DefaultBuild,
        mut cfg: crate::config::Build,
    ) -> Self {
        let mut cmake_command: Vec<String> = vec!["cmake".to_string()];
        cmake_command.append(&mut default_cfg.cmake_args.clone());
        cmake_command.append(&mut cfg.cmake_args);
        cmake_command.push("..".into());

        let mut package_envs: HashMap<String, String> = default_cfg
            .package_envs
            .clone()
            .iter()
            .map(|p| (p.key.clone(), p.value.clone()))
            .collect();

        for p in cfg.package_envs {
            package_envs.insert(p.key, p.value);
        }

        package_envs.insert("OUTPUT_DMG_PATH".to_string(), dmg_output_path.to_string());

        let repo_url = format!("https://github.com/{repo_owner}/{repo_name}");

        // TODO: this shouldn't be hardcoded
        let repo_dir: PathBuf = repo_dir.into();
        let build_dir = repo_dir.join(cfg.build_dir);
        let artifact_path = build_dir.join(dmg_output_path);

        Self {
            github_client,

            repo_dir,
            build_dir,
            artifact_path,

            asset_name: cfg.asset_name,

            repo_url,
            repo_owner,
            repo_name,

            release_id: branch.release_id,

            cmake_command: cmake_command.join(" "),
            package_envs,
        }
    }

    async fn build_asset(&self) -> anyhow::Result<()> {
        /*
        if let Err(e) = std::fs::remove_dir_all(self.repo_dir) {
            // Don't error out if the directory we want to delete doesn't exist
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(anyhow::anyhow!(e));
            }
        }
        */

        if let Ok(repo) = git2::Repository::open(&self.repo_dir) {
            info!("Using already-existing repo");
            {
                let mut remote = repo.find_remote("origin")?;
                info!("Repo remote: {:?}", remote.name());
                dbg!(remote.url());

                let fetch_commit = crate::git::fetch(&repo, &mut remote)?;

                // repo.merge(&[&fetch_commit], None, None)?;

                crate::git::merge(&repo, "master", fetch_commit)?;
            }
        } else {
            info!("Cloning to {:?}", self.repo_dir);
            std::fs::create_dir_all(&self.repo_dir)?;
            git2::Repository::clone_recurse(&self.repo_url, &self.repo_dir)?;
            info!("Cloned to {:?}", self.repo_dir);
        }

        // Build chatterino 4Head

        if let Err(e) = std::fs::remove_dir_all(&self.build_dir) {
            // Don't error out if the directory we want to delete doesn't exist
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(anyhow::anyhow!(e));
            }
        }

        std::fs::create_dir_all(&self.build_dir)?;
        std::env::set_current_dir(&self.build_dir)?;

        run_command(&self.cmake_command, None).await?;

        run_command("make -j8", None).await?;

        run_command("../.CI/CreateDMG.sh", Some(&self.package_envs)).await?;

        Ok(())
    }

    async fn delete_old_asset(&self) -> anyhow::Result<()> {
        // 1. Delete the macOS asset if it already exists
        let old_macos_release_asset = github::find_macos_asset(
            self.github_client.clone(),
            &self.repo_owner,
            &self.repo_name,
            self.release_id,
            &self.asset_name,
        )
        .await
        .context("Finding macOS asset")?;

        if let Some(asset_id) = old_macos_release_asset {
            github::delete_github_asset(
                self.github_client.clone(),
                &self.repo_owner,
                &self.repo_name,
                asset_id,
            )
            .await
            .context("Deleting macOS asset")?;
        }
        Ok(())
    }

    async fn upload_asset(&self) -> anyhow::Result<UploadReleaseAssetRoot> {
        // TODO: Add retry mechanics
        let release_asset = github::upload_asset_to_github_release(
            self.github_client.clone(),
            &self.repo_owner,
            &self.repo_name,
            self.release_id,
            &self.artifact_path,
            &self.asset_name,
        )
        .await
        .context("Uploading macOS asset")?;

        Ok(release_asset)
    }

    // TODO: This should include commit hash
    // TODO: This should fire off a build event into a queue instead of just immediately building
    pub async fn build(&self) -> anyhow::Result<()> {
        self.build_asset().await.context("Building asset")?;

        self.delete_old_asset()
            .await
            .context("Deleting old asset")?;

        self.upload_asset().await.context("Uploading asset")?;

        info!("Done!");

        Ok(())
    }
}
