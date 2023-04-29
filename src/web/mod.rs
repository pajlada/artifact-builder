use std::sync::Mutex;

use actix_web::{
    web::{self, Data},
    App, HttpServer,
};
use tokio::task::AbortHandle;
use tracing_actix_web::TracingLogger;

#[allow(unused)]
use tracing::log::*;

mod ping;
mod push;
mod span_builder;

pub async fn start_server(
    cfg: crate::config::Config,
    pipelines: crate::build::Pipelines,
    github_client: reqwest::Client,
) -> Result<(), std::io::Error> {
    let github_client = Data::new(github_client);
    let web_cfg = Data::new(cfg.clone());
    let web_base_url = cfg.web.base_url.clone();
    let pipelines = Data::new(pipelines);
    let current_job: Data<Mutex<Option<AbortHandle>>> = Data::new(Mutex::new(None));

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
                    .service(push::push)
                    .service(ping::ping),
            )
    });

    for bind in &cfg.web.bind {
        server = server.bind(bind)?
    }

    server.run().await
}
