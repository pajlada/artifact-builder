use actix_web::web::Data;
use anyhow::{anyhow, Context};
use git2::Repository;

use std::{
    collections::HashMap,
    ffi::OsStr,
    path::PathBuf,
    process::{ExitStatus, Stdio},
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command as TokioCommand,
};
use tokio_stream::StreamExt;

use tracing::log::*;

use crate::{git, github};

pub async fn build_and_upload_asset(
    github_client: Data<reqwest::Client>,
    repo_owner: String,
    repo_name: String,
    release_id: u64,
) -> anyhow::Result<()> {
    info!("Start build");
    let (artifact_path, asset_name) = start_build("/tmp/artifact-builder", &repo_owner, &repo_name)
        .await
        .context("Failed building")?;
    info!("Finished building - the build exists at {artifact_path:?}!");

    // 1. Delete the macOS asset if it already exists
    let old_macos_release_asset = github::find_macos_asset(
        github_client.clone(),
        &repo_owner,
        &repo_name,
        release_id,
        &asset_name,
    )
    .await
    .context("Finding macOS asset")?;

    if let Some(asset_id) = old_macos_release_asset {
        github::delete_github_asset(github_client.clone(), &repo_owner, &repo_name, asset_id)
            .await
            .context("Deleting macOS asset")?;
    }

    let release_asset = github::upload_asset_to_github_release(
        github_client,
        &repo_owner,
        &repo_name,
        release_id,
        artifact_path,
        &asset_name,
    )
    .await
    .context("Uploading macOS asset")?;

    info!(
        "Successfully uploaded release asset {asset_name} to {}",
        release_asset.browser_download_url
    );

    Ok(())
}

#[tracing::instrument(skip())]
async fn run_command<Cmd>(command: Cmd, envs: Option<HashMap<&str, &str>>) -> anyhow::Result<()>
where
    Cmd: AsRef<OsStr> + std::fmt::Debug,
{
    let mut cmd = TokioCommand::new("sh");

    cmd.arg("-c");
    cmd.arg(command);

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    if let Some(envs) = envs {
        cmd.envs(envs);
    }

    let mut child = cmd.spawn()?;

    let stdout = child.stdout.take().unwrap();
    let stdout_reader = BufReader::new(stdout).lines();
    let mut stdout_reader_stream = tokio_stream::wrappers::LinesStream::new(stdout_reader);

    let stderr = child.stderr.take().unwrap();
    let stderr_reader = BufReader::new(stderr).lines();
    let mut stderr_reader_stream = tokio_stream::wrappers::LinesStream::new(stderr_reader);

    let handle: tokio::task::JoinHandle<Result<ExitStatus, std::io::Error>> =
        tokio::spawn(async move { child.wait().await });

    loop {
        tokio::select! {
            Some(Ok(line)) = stdout_reader_stream.next() => {
                info!("stdout: {line:?}");
            }
            Some(Ok(line)) = stderr_reader_stream.next() => {
                info!("stderr: {line:?}");
            }
            else => {
                break;
            }
        }
    }

    let status = handle.await??;

    if let Some(code) = status.code() {
        if code == 0 {
            Ok(())
        } else {
            Err(anyhow!("Process exited with status {code}"))
        }
    } else {
        Err(anyhow!("Process exited without a status code?"))
    }
}

async fn start_build(
    clone_dir_str: &str,
    repo_owner: &str,
    repo_name: &str,
) -> anyhow::Result<(PathBuf, String)> {
    let clone_dir = std::path::Path::new(clone_dir_str);
    let url = format!("https://github.com/{repo_owner}/{repo_name}");

    /*
    if let Err(e) = std::fs::remove_dir_all(clone_dir) {
        // Don't error out if the directory we want to delete doesn't exist
        if e.kind() != std::io::ErrorKind::NotFound {
            return Err(anyhow::anyhow!(e));
        }
    }
    */

    {
        let r = Repository::open(clone_dir);
        if let Ok(repo) = r {
            info!("Using already-existing repo");
            {
                let mut remote = repo.find_remote("origin")?;
                info!("Repo remote: {:?}", remote.name());
                dbg!(remote.url());

                let fetch_commit = git::fetch(&repo, &mut remote)?;

                // repo.merge(&[&fetch_commit], None, None)?;

                git::merge(&repo, "master", fetch_commit)?;
            }
            // TODO: update repo somehow
            repo
        } else {
            info!("Cloning to {clone_dir:?}");
            std::fs::create_dir_all(clone_dir)?;
            let repo = Repository::clone_recurse(&url, clone_dir)?;
            info!("Cloned to {clone_dir:?}");
            repo
        }
    };

    // Build chatterino 4Head
    let build_dir = clone_dir.join("build");

    if let Err(e) = std::fs::remove_dir_all(&build_dir) {
        // Don't error out if the directory we want to delete doesn't exist
        if e.kind() != std::io::ErrorKind::NotFound {
            return Err(anyhow::anyhow!(e));
        }
    }

    std::fs::create_dir_all(&build_dir)?;
    std::env::set_current_dir(&build_dir)?;

    let qt_version = "6.5.0";
    let qt_dir = "/opt/qt/6.5.0/macos";
    let openssl_root_dir = "/opt/homebrew/opt/openssl@1.1";

    let mut cmake_arguments: Vec<String> = vec![
        "-DUSE_PRECOMPILED_HEADERS=OFF".into(),
        format!("-DCMAKE_PREFIX_PATH={qt_dir}"),
        format!("-DOPENSSL_ROOT_DIR={openssl_root_dir}"),
    ];

    cmake_arguments.push("-DBUILD_WITH_QT6=ON".into());

    let cmake_command =
        format!("cmake -DUSE_PRECOMPILED_HEADERS=OFF -DBUILD_WITH_QT6=ON -DCMAKE_PREFIX_PATH={qt_dir} -DOPENSSL_ROOT_DIR={openssl_root_dir} ..");

    run_command(cmake_command, None).await?;

    let build_command = "make -j8";
    run_command(build_command, None).await?;

    let create_dmg_command = format!("../.CI/CreateDMG.sh {qt_version}");
    let mut envs: HashMap<&str, &str> = HashMap::new();
    envs.insert("Qt6_DIR", qt_dir);
    envs.insert("SKIP_VENV", "1");
    run_command(create_dmg_command, Some(envs)).await?;

    // TODO: programmatically find this
    let dmg_output = format!("chatterino-macos-Qt-{qt_version}.dmg");

    Ok((build_dir.join(&dmg_output), dmg_output))
}
