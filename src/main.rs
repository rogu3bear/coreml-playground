#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::{routing::get, Router};
    use leptos::logging::log;
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use coreml_playground::app::*;
    use coreml_playground::server::{
        middleware::{self, RateLimiter},
        model_registry::ModelRegistry,
        session_store::SessionStore,
    };
    use axum::middleware as axum_middleware;
    use std::sync::Arc;
    use axum::extract::Extension;

    let conf = get_configuration(None).unwrap();
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    let routes = generate_route_list(App);

    // Initialize services
    let model_dir = std::env::var("COREML_MODELS_DIR")
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            format!("{}/Models", home)
        });
    let registry = Arc::new(ModelRegistry::new(&model_dir));
    let session_store = Arc::new(SessionStore::new("data/sessions.db").expect("Failed to init session store"));

    // Start model directory watcher
    let watcher_registry = registry.clone();
    tokio::spawn(async move {
        watcher_registry.watch().await;
    });

    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/ws/inference", get(coreml_playground::server::inference::ws_handler))
        .leptos_routes(&leptos_options, routes, {
            let leptos_options = leptos_options.clone();
            move || shell(leptos_options.clone())
        })
        .fallback(leptos_axum::file_and_error_handler(shell))
        .layer(axum_middleware::from_fn(middleware::api_key_guard))
        .layer(axum_middleware::from_fn(middleware::rate_limit))
        .layer(Extension(RateLimiter::new(120, 60)))
        .layer(Extension(registry))
        .layer(Extension(session_store))
        .with_state(leptos_options);

    log!("listening on http://{}", &addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service()).await.unwrap();
}

#[cfg(not(feature = "ssr"))]
pub fn main() {}
