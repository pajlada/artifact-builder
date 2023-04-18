#![allow(unused)]

use actix_web::{
    http::header::HeaderValue,
    web::{self, Bytes},
    App, HttpRequest, HttpResponse, HttpServer,
};
use git2::Repository;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tracing_actix_web::TracingLogger;

use tracing::log::*;

mod model;
mod span;

use crate::span::CustomRootSpanBuilder;

const WEBHOOK_SECRET: &[u8] = "penis123".as_bytes();

const MAIN_BRANCH_REF: &str = "refs/heads/master";
const REPO_FULL_NAME: &str = "pajlada/test";

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

#[tracing::instrument(skip(bytes, req))]
async fn new_build(req: HttpRequest, bytes: Bytes) -> actix_web::Result<actix_web::HttpResponse> {
    let signature = get_hub_signature(req.headers().get("x-hub-signature-256"))?;

    validate_hub_signature(signature, &bytes, WEBHOOK_SECRET)?;

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

    tracing::info!("make build job");

    Ok(HttpResponse::Ok().body("forsen"))
}

async fn start_build(build_dir: &str, repo_full_name: &str) -> anyhow::Result<()> {
    let url = format!("https://github.com/{repo_full_name}");

    if let Err(e) = std::fs::remove_dir_all(build_dir) {
        // Don't error out if the directory we want to delete doesn't exist
        if e.kind() != std::io::ErrorKind::NotFound {
            return Err(anyhow::anyhow!(e));
        }
    }

    std::fs::create_dir_all(build_dir)?;

    let repo = Repository::clone(&url, build_dir)?;

    info!("Cloned to {build_dir}");

    for mut submodule in repo.submodules()? {
        info!("Cloning submodule {:?}", submodule.name());
        submodule.update(true, None)?;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // start build
    start_build("/tmp/artifact-builder", "Chatterino/chatterino2").await?;

    /*
    HttpServer::new(|| {
        let tracing_logger = TracingLogger::<CustomRootSpanBuilder>::new();
        App::new()
            .wrap(tracing_logger)
            .wrap(actix_web::middleware::Logger::default())
            .service(web::resource("/new-build").to(new_build))
    })
    .bind("127.0.0.1:8000")?
    .run()
    .await?;
    */

    Ok(())
}
