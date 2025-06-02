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

    blah();

    pserve::server::App::default()
        .wasm(include_bytes!(
            "../target/wasm32-unknown-unknown/debug/hello_server.wasm"
        ))
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


fn blah() {
    use pserve::ui::*;
    let mut ui_state = pserve::ui::State::new();

    {
        let link = "https://google.com".to_string();
        ui_state.reset();
        ui_state.open_element(
            ElementKind::Text("I am an item".to_string()),
            Layout::default(),
            HtmlElementType::Link(link),
        );
        ui_state.close_element();
        ui_state.compute_layout();
    }

    let link = "alkdjfhklad".to_string();

    for i in 0..ui_state.elements.len {
        let e = &ui_state.elements.items[i].data;

        let element_type = e
            .user_data
            .and_then(|index| ui_state.fetch_user_data::<pserve::ui::HtmlElementType>(index as usize));

       let string = match &element_type {
            Some(HtmlElementType::Button) => format!("<button id={i} style=\""),
            Some(HtmlElementType::TextBox) => format!("<input id={i} style=\""),
            Some(HtmlElementType::Link(link)) => format!("<a id={i} href=\"{link}\" style=\""),
            None => format!("<div id={i} style=\""),
        };

       println!("{string}");
    }
}
