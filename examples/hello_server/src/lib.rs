#[cfg(target_arch = "wasm32")]
pub mod client;

#[cfg(not(target_arch = "wasm32"))]
use pserve::server::{Event, ToClientEvent};

use pserve::state::{MultipleValueUpdate, Stateful, Valuable};

use serde::{Deserialize, Serialize};
use std::hash::Hash;
use std::net::SocketAddr;

pub const NUMBER_OF_CHECKBOXES: usize = 100;

pub struct State {
    check_boxes: [bool; NUMBER_OF_CHECKBOXES * NUMBER_OF_CHECKBOXES],
    meme_list: Vec<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            check_boxes: [false; NUMBER_OF_CHECKBOXES * NUMBER_OF_CHECKBOXES],
            meme_list: vec![
                "React".to_string(),
                "Rust".to_string(),
                "Dioxus".to_string(),
                "Leptos".to_string(),
            ],
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientEvent {
    RenderComponent {
        component_name: String,
        dom_id: Option<u32>,
    },
    ToggleCheckBox {
        id: u32,
    },
    AddMeme {
        meme: String,
    },
}

#[derive(Clone, Copy)]
pub struct MySuperCoolSingleValueStateEvent;
impl pserve::state::Valuable<pserve::state::IsSingleValue> for MySuperCoolSingleValueStateEvent {}
impl pserve::state::Stateful for MySuperCoolSingleValueStateEvent {
    type Data = String;
    type Key = ();

    fn name() -> &'static str {
        "mySuperCoolSingleValueStateEvent"
    }
}

#[derive(Clone, Copy)]
pub struct CheckBoxStateEvent;
impl pserve::state::Stateful for CheckBoxStateEvent {
    type Data = Vec<bool>;
    type Key = u32;

    fn name() -> &'static str {
        "checkBoxes"
    }
}

#[derive(Clone, Copy)]
pub struct MemeListStateEvent;
impl pserve::state::Stateful for MemeListStateEvent {
    type Data = Vec<String>;
    type Key = u32;

    fn name() -> &'static str {
        "memeList"
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn request_full_state(state: &mut State, who: SocketAddr, name: String) -> Option<Event> {
    // Some(Event::ToSpecificClient {
    //     who,
    //     event: ToClientEvent::Custom {
    //         event: todo!(), //serde_json::to_value(FullState::from_name(state, name)).unwrap(),
    //     },
    // })

    match name.as_str() {
        "memeList" => Some(Event::ToSpecificClient {
            who,
            event: MemeListStateEvent::as_full_update(&state.meme_list),
        }),
        "checkBoxes" => Some(Event::ToSpecificClient {
            who,
            event: CheckBoxStateEvent::as_full_update(&state.check_boxes),
        }),
        "mySuperCoolSingleValueStateEvent" => Some(Event::ToSpecificClient {
            who,
            event: ToClientEvent::Custom {
                event: serde_json::to_value("Hello, I'm different".to_string()).unwrap(),
            },
        }),
        _ => None,
    }
}

#[cfg(not(target_arch = "wasm32"))]
// TODO: #[processor]
pub fn render_component_for_everyone(
    _: &mut State,
    _who: SocketAddr,
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
        params: None,
    }))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn toggle_check_box(
    state: &mut State,
    _who: SocketAddr,
    value: serde_json::Value,
) -> Option<Event> {
    pserve::server::tracing::info!("toggle_check_box: {:?}", value);
    let event: ClientEvent = serde_json::from_value(value).unwrap();

    let ClientEvent::ToggleCheckBox { id } = event else {
        return None;
    };

    state.check_boxes[id as usize] = !state.check_boxes[id as usize];

    Some(Event::ToAllClients(CheckBoxStateEvent::as_update(
        id,
        state.check_boxes[id as usize],
    )))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn add_meme(state: &mut State, _who: SocketAddr, value: serde_json::Value) -> Option<Event> {
    pserve::server::tracing::info!("add_meme: {:?}", value);
    let event: ClientEvent = serde_json::from_value(value).unwrap();

    let ClientEvent::AddMeme { meme } = event else {
        return None;
    };

    state.meme_list.push(meme.clone());

    Some(Event::ToAllClients(MemeListStateEvent::as_update(
        state.meme_list.len() as u32 - 1,
        meme,
    )))
}
