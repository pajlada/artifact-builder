use std::sync::Mutex;

use actix_web::{
    guard,
    web::{self, Data},
    App, HttpServer,
};
use tokio::task::AbortHandle;
use tracing_actix_web::TracingLogger;

#[allow(unused)]
use tracing::log::*;

mod middleware;
mod ping;
mod push;
mod span_builder;

use self::middleware::VerifyGithubSignatureFactory;

pub async fn start_server(
    cfg: crate::config::Config,
    pipelines: crate::build::Pipelines,
    github_client: reqwest::Client,
) -> anyhow::Result<()> {
    let github_client = Data::new(github_client);
    let web_cfg = Data::new(cfg.clone());
    let web_base_url = cfg.web.base_url.clone();
    let pipelines = Data::new(pipelines);
    let current_job: Data<Mutex<Option<AbortHandle>>> = Data::new(Mutex::new(None));

    if !cfg.github.verify_signature {
        warn!("Github signature verification is disabled");
    }

    let verify_signature =
        VerifyGithubSignatureFactory::new(cfg.github.verify_signature, &cfg.github.secret).unwrap();

    let mut server = HttpServer::new(move || {
        let tracing_logger = TracingLogger::<span_builder::SpanBuilder>::new();

        App::new()
            .app_data(github_client.clone())
            .app_data(web_cfg.clone())
            .app_data(pipelines.clone())
            .app_data(current_job.clone())
            .wrap(tracing_logger)
            .wrap(actix_web::middleware::Logger::default())
            .service(
                web::scope(&web_base_url)
                    .route(
                        "/push",
                        web::post()
                            .wrap(verify_signature.clone())
                            .guard(guard::Header("x-github-event", "push"))
                            .to(push::on_push),
                    )
                    .route(
                        "/push",
                        web::post()
                            .wrap(verify_signature.clone())
                            .guard(guard::Header("x-github-event", "ping"))
                            .to(push::on_ping),
                    )
                    .service(ping::ping),
            )
    });

    for bind in &cfg.web.bind {
        server = server.bind(bind)?
    }

    server.run().await?;

    Ok(())
}
