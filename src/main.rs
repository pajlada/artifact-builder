use actix_web::{
    http::header::HeaderValue,
    web::{self, Bytes},
    App, HttpRequest, HttpResponse, HttpServer,
};
use anyhow::{anyhow, Context};
use git2::Repository;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::str::FromStr;
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
use tracing_actix_web::TracingLogger;

use tracing::log::*;

mod git;
mod model;
mod span;

use crate::{
    model::{GetReleaseRoot, UploadReleaseAssetRoot},
    span::CustomRootSpanBuilder,
};

const VERIFY_GITHUB_SIGNATURE: bool = false;

const USER_AGENT: &str = "chatterino-macos-artifact-builder 0.1.0";

const WEBHOOK_SECRET: &[u8] = "penis123".as_bytes();
const REPO_OWNER: &str = "pajlada";
const REPO_NAME: &str = "chatterino2";
const REPO_FULL_NAME: &str = const_format::formatcp!("{REPO_OWNER}/{REPO_NAME}");

// The release we want to upload our asset to
const RELEASE_ID: u64 = 82423741;

const MAIN_BRANCH_REF: &str = "refs/heads/master";

lazy_static::lazy_static! {
    static ref GITHUB_CLIENT: reqwest::Client = {
        let token = std::env::var("GITHUB_TOKEN").expect("Must have a GITHUB_TOKEN environment variable set");

        let authorization_value: String = format!("Bearer {token}");

        let mut default_headers = reqwest::header::HeaderMap::new();
        default_headers.insert("User-Agent", reqwest::header::HeaderValue::from_static(USER_AGENT));
        default_headers.insert("Accept", reqwest::header::HeaderValue::from_static("application/vnd.github+json"));
        default_headers.insert("Authorization", reqwest::header::HeaderValue::from_str(authorization_value.as_str()).unwrap());

        reqwest::Client::builder().default_headers(default_headers).build().unwrap()
    };
}

fn get_hub_signature(hv: Option<&HeaderValue>) -> Result<Vec<u8>, actix_web::Error> {
    match hv {
        Some(v) => {
            let val = v
                .to_str()
                .map_err(actix_web::error::ErrorBadRequest)?
                .strip_prefix("sha256=")
                .ok_or_else(|| actix_web::error::ErrorBadRequest("missing prefix"))?;

            hex::decode(val).map_err(actix_web::error::ErrorBadRequest)
        }
        None => Err(actix_web::error::ErrorBadRequest(
            "missing signature header",
        )),
    }
}

fn validate_hub_signature(
    hub_signature: Vec<u8>,
    bytes: &Bytes,
    secret: &[u8],
) -> Result<(), actix_web::Error> {
    let mut hasher =
        Hmac::<Sha256>::new_from_slice(secret).map_err(actix_web::error::ErrorBadRequest)?;

    hasher.update(bytes);

    hasher
        .verify_slice(&hub_signature)
        .map_err(actix_web::error::ErrorUnauthorized)?;

    Ok(())
}

async fn build_and_upload_asset() -> anyhow::Result<()> {
    info!("Start build");
    let (artifact_path, asset_name) =
        start_build("/tmp/artifact-builder", "Chatterino/chatterino2")
            .await
            .context("Failed building")?;
    info!("Finished building - the build exists at {artifact_path:?}!");

    // 1. Delete the macOS asset if it already exists
    let old_macos_release_asset = find_macos_asset(REPO_OWNER, REPO_NAME, RELEASE_ID, &asset_name)
        .await
        .context("Finding macOS asset")?;

    if let Some(asset_id) = old_macos_release_asset {
        delete_github_asset(REPO_OWNER, REPO_NAME, asset_id)
            .await
            .context("Deleting macOS asset")?;
    }

    let release_asset = upload_asset_to_github_release(
        REPO_OWNER,
        REPO_NAME,
        RELEASE_ID,
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

#[tracing::instrument(skip(bytes, req))]
async fn new_build(req: HttpRequest, bytes: Bytes) -> actix_web::Result<actix_web::HttpResponse> {
    if VERIFY_GITHUB_SIGNATURE {
        let signature = get_hub_signature(req.headers().get("x-hub-signature-256"))?;

        validate_hub_signature(signature, &bytes, WEBHOOK_SECRET)?;
    }

    // TODO: specify commit
    tokio::spawn(async {
        let res = build_and_upload_asset().await;

        if let Err(e) = res {
            info!("Error building/uploading asset: {e}");
        }
    });

    let body: model::Root =
        serde_json::from_slice(&bytes).map_err(actix_web::error::ErrorBadRequest)?;

    if body.push_ref != MAIN_BRANCH_REF {
        return Ok(HttpResponse::Ok().body(format!(
            "Push event is not for the main branch '{}",
            MAIN_BRANCH_REF
        )));
    }

    if body.repository.full_name != REPO_FULL_NAME {
        return Ok(HttpResponse::Ok().body(format!(
            "Push event is not for the correct repo '{}",
            REPO_FULL_NAME
        )));
    }

    Ok(HttpResponse::Ok().body("forsen"))
}

#[tracing::instrument(skip())]
async fn run_command<Cmd>(command: Cmd, envs: Option<HashMap<String, String>>) -> anyhow::Result<()>
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
    repo_full_name: &str,
) -> anyhow::Result<(PathBuf, String)> {
    let clone_dir = std::path::Path::new(clone_dir_str);
    let url = format!("https://github.com/{repo_full_name}");

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

                let fetch_commit = git::fetch(&repo, &["master"], &mut remote)?;

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

    let qt_version = "5.15.8";
    let qt_dir = "/opt/homebrew/opt/qt@5";
    let openssl_root_dir = "/opt/homebrew/opt/openssl@1.1";

    let cmake_command =
        format!("cmake -DUSE_PRECOMPILED_HEADERS=OFF -DCMAKE_PREFIX_PATH={qt_dir} -DOPENSSL_ROOT_DIR={openssl_root_dir} ..");

    run_command(cmake_command, None).await?;

    let build_command = "make -j8";
    run_command(build_command, None).await?;

    // TODO: Use codesign
    let create_dmg_command = format!("../.CI/CreateDMG.sh {qt_version}");
    let mut envs: HashMap<String, String> = HashMap::new();
    envs.insert("Qt5_DIR".to_string(), qt_dir.to_string());
    run_command(create_dmg_command, Some(envs)).await?;

    let dmg_output = "chatterino-macos-Qt-5.15.8.dmg";

    Ok((build_dir.join(dmg_output), dmg_output.to_string()))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    HttpServer::new(|| {
        let tracing_logger = TracingLogger::<CustomRootSpanBuilder>::new();
        App::new()
            .wrap(tracing_logger)
            .wrap(actix_web::middleware::Logger::default())
            .service(web::resource("/new-build").to(new_build))
    })
    .bind("0.0.0.0:8000")?
    .run()
    .await?;

    Ok(())
}

async fn find_macos_asset(
    owner: &str,
    repo: &str,
    release_id: u64,
    asset_name: &str,
) -> anyhow::Result<Option<u64>> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/{release_id}");
    let res = GITHUB_CLIENT.get(url).send().await?.error_for_status()?;

    let release: GetReleaseRoot = res.json().await?;

    let macos_asset = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .map(|asset| asset.id);

    Ok(macos_asset)
}

async fn delete_github_asset(owner: &str, repo: &str, asset_id: u64) -> anyhow::Result<()> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/assets/{asset_id}");

    GITHUB_CLIENT.delete(url).send().await?.error_for_status()?;

    Ok(())
}

async fn upload_asset_to_github_release(
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

    let res: UploadReleaseAssetRoot = GITHUB_CLIENT
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
