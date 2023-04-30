use std::{
    future::{ready, Ready},
    rc::Rc,
};

use actix_http::h1;
use actix_web::{
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    web::Bytes,
};
use futures_util::{future::LocalBoxFuture, FutureExt};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use tracing::log::*;

pub struct VerifyGithubSignature<S> {
    service: Rc<S>,
    validate_secret: bool,
    hasher: Hmac<Sha256>,
}

impl<S, B> Service<ServiceRequest> for VerifyGithubSignature<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    actix_service::forward_ready!(service);

    fn call(&self, mut req: ServiceRequest) -> Self::Future {
        // Clone the Rc pointers so we can move them into the async block.
        let srv = self.service.clone();
        let mut hasher = self.hasher.clone();
        // let auth_data = self.auth_data.clone();

        let validate_secret = self.validate_secret;

        async move {
            if validate_secret {
                info!("Validating secret");
                let signature_header = req
                    .headers()
                    .get("x-hub-signature-256")
                    .ok_or_else(|| actix_web::error::ErrorBadRequest("missing signature header"))?;

                let signature_bytes = hex::decode(
                    signature_header
                        .to_str()
                        .map_err(actix_web::error::ErrorBadRequest)?
                        .strip_prefix("sha256=")
                        .ok_or_else(|| actix_web::error::ErrorBadRequest("missing prefix"))?,
                )
                .map_err(actix_web::error::ErrorBadRequest)?;

                let body = req.extract::<Bytes>().await.unwrap();

                hasher.update(&body);

                hasher
                    .verify_slice(&signature_bytes)
                    .map_err(actix_web::error::ErrorUnauthorized)?;

                // re-insert body back into request to be used by handlers
                req.set_payload(bytes_to_payload(body));
            } else {
                info!("skipping secret validation");
            }

            let res = srv.call(req).await?;

            Ok(res)
        }
        .boxed_local()
    }
}

#[derive(Clone)]
pub struct VerifyGithubSignatureFactory {
    validate_secret: bool,
    hasher: Hmac<Sha256>,
}

impl VerifyGithubSignatureFactory {
    pub fn new(validate_secret: bool, secret: &str) -> actix_web::Result<Self> {
        let hasher = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(actix_web::error::ErrorBadRequest)?;

        Ok(Self {
            validate_secret,
            hasher,
        })
    }
}

impl<S, B> Transform<S, ServiceRequest> for VerifyGithubSignatureFactory
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Transform = VerifyGithubSignature<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(VerifyGithubSignature {
            service: Rc::new(service),
            validate_secret: self.validate_secret,
            hasher: self.hasher.clone(),
        }))
    }
}

fn bytes_to_payload(buf: Bytes) -> actix_web::dev::Payload {
    let (_, mut pl) = h1::Payload::create(true);
    pl.unread_data(buf);
    actix_web::dev::Payload::from(pl)
}
