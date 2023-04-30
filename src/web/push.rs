use std::sync::Mutex;

use actix_web::{web::Data, web::Json, HttpResponse};

use tokio::task::AbortHandle;
#[allow(unused)]
use tracing::log::*;

use crate::config;
use crate::github;

#[tracing::instrument(skip(cfg, pipelines, current_job, payload))]
pub async fn on_push(
    cfg: Data<config::Config>,
    pipelines: Data<crate::build::Pipelines>,
    current_job: Data<Mutex<Option<AbortHandle>>>,
    payload: Json<github::model::Root>,
) -> actix_web::Result<actix_web::HttpResponse> {
    info!("On push");

    let repo_owner = cfg.github.repo_owner.clone();
    let repo_name = cfg.github.repo_name.clone();
    let repo_full_name = format!("{}/{}", repo_owner, repo_name);

    if payload.repository.full_name != repo_full_name {
        return Err(actix_web::error::ErrorBadRequest(format!(
            "Push event is not for the correct repo '{}",
            repo_full_name
        )));
    }

    let stripped_branch_name = match payload.push_ref.strip_prefix("refs/heads/") {
        Some(stripped_branch_name) => stripped_branch_name,

        None => {
            return Ok(HttpResponse::Ok().body(format!(
                "Ignoring build for non-branch push {}",
                payload.push_ref
            )));
        }
    };

    match pipelines.get(stripped_branch_name) {
        Some(pipelines) => {
            let pipelines = pipelines.clone();
            if pipelines.is_empty() {
                info!("No push events registered for {stripped_branch_name}");
                return Ok(HttpResponse::Ok()
                    .body(format!("The branch {stripped_branch_name} is not handled")));
            }

            let num_pipelines = pipelines.len();

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

            Ok(HttpResponse::Ok().body(format!("Spun up {num_pipelines} builds")))
        }
        None => Ok(
            HttpResponse::Ok().body(format!("The branch {stripped_branch_name} is not handled"))
        ),
    }
}

#[tracing::instrument()]
pub async fn on_ping() -> actix_web::Result<actix_web::HttpResponse> {
    info!("On ping");

    Ok(HttpResponse::Ok().body("pong"))
}
