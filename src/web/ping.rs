use actix_web::{get, HttpResponse};

#[allow(unused)]
use tracing::log::*;

#[tracing::instrument()]
#[get("/ping")]
pub async fn ping() -> actix_web::Result<actix_web::HttpResponse> {
    info!("ping");

    Ok(HttpResponse::Ok().body("pong"))
}
