use pserve::server::tokio;
use pserve::server::tracing;
use pserve::server::tracing_subscriber::{self, layer::SubscriberExt, util::SubscriberInitExt};

use hello_server::{add_meme, render_component_for_everyone, toggle_check_box};

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
        .state_processor(hello_server::request_full_state)
        .add_processor(render_component_for_everyone)
        .add_processor(toggle_check_box)
        .add_processor(add_meme)
        .route("/", "home_page")
        .route("/meme_list", "meme_list")
        .route("/server_communicator", "server_communicator")
        .route("/checkboxes", "checkboxes")
        .state(hello_server::State::default())
        .serve()
        .await
        .unwrap();
}
