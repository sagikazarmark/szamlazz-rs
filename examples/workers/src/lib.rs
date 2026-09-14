//! Isolated initial-ordinary-invoice Workers endpoint; see the adjacent README.

use http_body_util::BodyExt as _;
use restate_szamlazz::{
    Agent, Order,
    account::{Accounts, StaticConfig, StaticResolver},
    config::WorkerConfig,
    restate_sdk::{
        self,
        endpoint::{HandleOptions, ProtocolMode},
        service::IntoServiceDefinition as _,
    },
};

#[cfg(feature = "acceptance-tests")]
mod acceptance;

fn invalid(message: &'static str) -> worker::Error {
    worker::Error::RustError(message.into())
}

#[worker::event(fetch)]
async fn fetch(
    request: worker::HttpRequest,
    env: worker::Env,
    _: worker::Context,
) -> worker::Result<worker::HttpResponse> {
    // SDK's WASM input drain has no timer. Bound the entire request/SDK output
    // lifetime and drop its future on expiry. No host timeout clears Order state.
    use futures_util::future::{Either, select};
    let timeout = gloo_timers::future::TimeoutFuture::new(10 * 60 * 1000);
    match select(std::pin::pin!(serve(request, env)), std::pin::pin!(timeout)).await {
        Either::Left((result, _)) => result,
        Either::Right(_) => {
            worker::Response::error("endpoint exchange deadline exceeded", 504)?.try_into()
        }
    }
}

async fn serve(
    request: worker::HttpRequest,
    env: worker::Env,
) -> worker::Result<worker::HttpResponse> {
    let static_config: StaticConfig =
        serde_json::from_str(&env.secret("SZAMLAZZ_ACCOUNTS")?.to_string())
            .map_err(|_| invalid("invalid account configuration"))?;
    let accounts = Accounts::from(
        StaticResolver::try_from(static_config)
            .map_err(|_| invalid("invalid account configuration"))?,
    );
    #[cfg(feature = "acceptance-tests")]
    if request.uri().path() == "/transport-probe" {
        return acceptance::probe(&accounts, &request).await;
    }
    #[cfg(feature = "acceptance-tests")]
    let accounts = acceptance::accounts(accounts, &env);

    let namespace = env
        .var("SZAMLAZZ_NAMESPACE")?
        .to_string()
        .parse()
        .map_err(|_| invalid("invalid namespace"))?;
    let config = WorkerConfig::new(namespace);
    #[cfg(feature = "acceptance-tests")]
    let config = acceptance::config(config);
    let config = config
        .validate()
        .map_err(|_| invalid("invalid worker configuration"))?;
    let order = Order::from_parts(accounts.clone(), config.clone())
        .experimental_request_response()
        .into_service_definition();
    #[cfg(feature = "acceptance-tests")]
    let order = order.options(
        restate_sdk::endpoint::ServiceOptions::default().handler(
            "create_invoice",
            restate_sdk::endpoint::HandlerOptions::default()
                .retry_policy_initial_interval(std::time::Duration::from_millis(100))
                .retry_policy_max_interval(std::time::Duration::from_millis(100))
                .retry_policy_max_attempts(2)
                .retry_policy_pause_on_max_attempts(),
        ),
    );
    // Required: a trusted ingress gateway selects scope, and only the signed
    // Restate runtime can address this endpoint. The ingress gateway must
    // separately authorize operator access to observe_unresolved and recover.
    let identity = env.var("RESTATE_IDENTITY_KEY")?.to_string();
    let endpoint = restate_sdk::prelude::Endpoint::builder()
        .bind(order)
        .bind(Agent::from_parts(accounts, config).experimental_request_response())
        .identity_key(&identity)
        .map_err(|_| invalid("invalid request identity key"))?
        .build();
    // Consume the entire protocol output in this event's lifetime. No detached
    // SDK execution or waitUntil continuation is part of this hosting model.
    let response = endpoint.handle_with_options(
        request,
        HandleOptions {
            protocol_mode: ProtocolMode::RequestResponse,
        },
    );
    let (parts, body) = response.into_parts();
    let bytes = body
        .collect()
        .await
        .map_err(|_| invalid("SDK output failed"))?
        .to_bytes();
    let body: worker::HttpResponse = worker::Response::from_bytes(bytes.to_vec())?.try_into()?;
    Ok(worker::HttpResponse::from_parts(parts, body.into_body()))
}
