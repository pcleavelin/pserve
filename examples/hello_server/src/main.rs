use pserve::server::tokio;
use pserve::server::tracing;
use pserve::server::tracing_subscriber::{self, layer::SubscriberExt, util::SubscriberInitExt};

use hello_server::render_component_for_everyone;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Hello, world!");

    pserve::server::App::default()
        .add_processor(render_component_for_everyone)
        .route("/", "home_page")
        .route("/meme_list", "meme_list")
        .route("/server_communicator", "server_communicator")
        .serve()
        .await
        .unwrap();
}
