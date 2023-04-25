use actix_web::{
    http::header::HeaderValue,
    post,
    web::{Bytes, Data},
    HttpRequest, HttpResponse,
};

use hmac::{Hmac, Mac};
use sha2::Sha256;

#[allow(unused)]
use tracing::log::*;

use crate::config;
use crate::github;
// TODO: this should be some worker that's passed in
use crate::build::build_and_upload_asset;

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

#[tracing::instrument(skip(bytes, req))]
#[post("/push")]
async fn push(
    req: HttpRequest,
    bytes: Bytes,
    github_client: Data<reqwest::Client>,
    cfg: Data<config::Config>,
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

    // Figure out which release this push event should be pushed to
    let branch = cfg
        .github
        .branches
        .iter()
        .cloned()
        .find(|b| body.push_ref == format!("refs/heads/{}", b.name));

    match branch {
        Some(branch) => {
            // TODO: make sure to build, clone, push to the correct branch
            tokio::spawn(async move {
                let repo_owner = cfg.github.repo_owner.clone();
                let repo_name = cfg.github.repo_name.clone();

                let res =
                    build_and_upload_asset(github_client, repo_owner, repo_name, branch.release_id)
                        .await;

                if let Err(e) = res {
                    info!("Error building/uploading asset: {e:?}");
                }
            });
        }
        None => {
            return Ok(HttpResponse::Ok().body(format!(
                "No release configured for branch {}",
                body.push_ref
            )));
        }
    }

    if body.repository.full_name != repo_full_name {
        return Ok(HttpResponse::Ok().body(format!(
            "Push event is not for the correct repo '{}",
            repo_full_name
        )));
    }

    Ok(HttpResponse::Ok().body("forsen"))
}
