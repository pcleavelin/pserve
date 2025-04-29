#[cfg(target_arch = "wasm32")]
pub mod client;

#[cfg(not(target_arch = "wasm32"))]
use pserve::server::{Event, ToClientEvent};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientEvent {
    RenderComponent {
        component_name: String,
        dom_id: Option<u32>,
    },
}

#[cfg(not(target_arch = "wasm32"))]
// TODO: #[processor]
pub fn render_component_for_everyone(
    value: serde_json::Value, /* event: ClientEvent */
) -> Option<Event> {
    pserve::server::tracing::info!("{:?}", value);
    let event: ClientEvent = serde_json::from_value(value).unwrap();

    let ClientEvent::RenderComponent { component_name, .. } = event else {
        return None;
    };

    Some(Event::ToAllClients(ToClientEvent::RenderComponent {
        component_name: component_name.clone(),
        dom_id: None,
    }))
}
