use actix_web::web::Data;
use std::path::PathBuf;
use std::str::FromStr;

pub mod client;
pub mod model;

use self::model::{GetReleaseRoot, UploadReleaseAssetRoot};

#[allow(unused)]
use tracing::log::*;

pub async fn find_macos_asset(
    github_client: Data<reqwest::Client>,
    owner: &str,
    repo: &str,
    release_id: u64,
    asset_name: &str,
) -> anyhow::Result<Option<u64>> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/{release_id}");
    let res = github_client.get(url).send().await?.error_for_status()?;

    let release: GetReleaseRoot = res.json().await?;

    let macos_asset = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .map(|asset| asset.id);

    Ok(macos_asset)
}

pub async fn delete_github_asset(
    github_client: Data<reqwest::Client>,
    owner: &str,
    repo: &str,
    asset_id: u64,
) -> anyhow::Result<()> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/assets/{asset_id}");

    github_client.delete(url).send().await?.error_for_status()?;

    Ok(())
}

pub async fn upload_asset_to_github_release(
    github_client: Data<reqwest::Client>,
    owner: &str,
    repo: &str,
    release_id: u64,
    path_to_file: PathBuf,
    asset_name: &str,
) -> anyhow::Result<UploadReleaseAssetRoot> {
    let release_upload_url =
        format!("https://uploads.github.com/repos/{owner}/{repo}/releases/{release_id}/assets");
    let mut release_upload_url = url::Url::from_str(&release_upload_url)?;
    release_upload_url.set_query(Some(format!("{}={}", "name", asset_name).as_str()));
    // println!("upload_url: {}", release_upload_url);
    let file_size = std::fs::metadata(&path_to_file)?.len();
    println!(
        "file_size: {}. It can take some time to upload. Wait...",
        file_size
    );
    let file = tokio::fs::File::open(path_to_file).await?;

    let res: UploadReleaseAssetRoot = github_client
        .post(release_upload_url)
        .header("Content-Type", "application/octet-stream")
        .header("Content-Length", file_size.to_string())
        .body(file)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(res)
}
