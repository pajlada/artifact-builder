use std::sync::Mutex;

use actix_web::{
    http::header::HeaderValue,
    post,
    web::{Bytes, Data},
    HttpRequest, HttpResponse,
};

use hmac::{Hmac, Mac};
use sha2::Sha256;

use tokio::task::AbortHandle;
#[allow(unused)]
use tracing::log::*;

use crate::config;
use crate::github;

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
    secret: &str,
) -> Result<(), actix_web::Error> {
    let mut hasher = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(actix_web::error::ErrorBadRequest)?;

    hasher.update(bytes);

    hasher
        .verify_slice(&hub_signature)
        .map_err(actix_web::error::ErrorUnauthorized)?;

    Ok(())
}

#[tracing::instrument(skip(bytes, req, cfg, pipelines))]
#[post("/push")]
async fn push(
    req: HttpRequest,
    bytes: Bytes,
    cfg: Data<config::Config>,
    pipelines: Data<crate::build::Pipelines>,
    current_job: Data<Mutex<Option<AbortHandle>>>,
) -> actix_web::Result<actix_web::HttpResponse> {
    if cfg.github.verify_signature {
        let signature = get_hub_signature(req.headers().get("x-hub-signature-256"))?;

        validate_hub_signature(signature, &bytes, &cfg.github.secret)?;
    }

    let repo_owner = cfg.github.repo_owner.clone();
    let repo_name = cfg.github.repo_name.clone();
    let repo_full_name = format!("{}/{}", repo_owner, repo_name);

    let body: github::model::Root =
        serde_json::from_slice(&bytes).map_err(actix_web::error::ErrorBadRequest)?;

    if body.repository.full_name != repo_full_name {
        return Ok(HttpResponse::Ok().body(format!(
            "Push event is not for the correct repo '{}",
            repo_full_name
        )));
    }

    let stripped_branch_name = body.push_ref.strip_prefix("refs/heads/").ok_or_else(|| {
        actix_web::error::ErrorBadRequest(format!(
            "push_ref doesn't start with refs/heads/ '{}'",
            body.push_ref
        ))
    })?;

    let pipelines = pipelines
        .get(stripped_branch_name)
        .ok_or_else(|| {
            actix_web::error::ErrorBadRequest("No pipeline found {stripped_branch_name}")
        })?
        .clone();

    if pipelines.is_empty() {
        info!("No push events registered for {stripped_branch_name}");
        return Ok(
            HttpResponse::Ok().body(format!("The branch {stripped_branch_name} is not handled"))
        );
    }

    let handle = tokio::spawn(async move {
        for p in pipelines {
            let res = p.build().await;

            if let Err(e) = res {
                info!("Error building/uploading asset: {e:?}");
            }
        }
    });

    let abort_handle = handle.abort_handle();

    let old_abort_handle = current_job.lock().unwrap().replace(abort_handle);

    if let Some(old_abort_handle) = old_abort_handle {
        info!("Aborting old job");
        old_abort_handle.abort();
    }

    Ok(HttpResponse::Ok().body("forsen"))
}
