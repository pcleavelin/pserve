use pserve::server::tokio;
use pserve::server::tracing_subscriber::{self, layer::SubscriberExt, util::SubscriberInitExt};

#[dotenvy::load]
#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    pserve::server::App::default()
        .wasm(include_bytes!(
            "../target/wasm32-unknown-unknown/debug/oauth.wasm"
        ))
        .cookie_processor(oauth::cookie_processor)
        .add_processor(oauth::discord_login)
        .route("/", "home_page")
        .route("/auth", "auth")
        .state(oauth::State::default())
        .serve()
        .await
        .unwrap();
}
