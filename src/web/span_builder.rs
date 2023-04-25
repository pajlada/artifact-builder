use actix_web::{
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
};
use tracing::{span, Level, Span};
use tracing_actix_web::{DefaultRootSpanBuilder, RootSpanBuilder};

pub struct SpanBuilder;

impl RootSpanBuilder for SpanBuilder {
    fn on_request_start(request: &ServiceRequest) -> Span {
        // let asd = request.version()
        let my_span = span!(Level::WARN, "forsen");

        my_span.record(
            "aaaaa",
            request.connection_info().peer_addr().unwrap_or("unknown"),
        );

        my_span
    }

    fn on_request_end<B: MessageBody>(
        span: Span,
        outcome: &Result<ServiceResponse<B>, actix_web::Error>,
    ) {
        // Capture the standard fields when the request finishes.
        DefaultRootSpanBuilder::on_request_end(span, outcome);
    }
}
