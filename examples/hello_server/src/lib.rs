#[cfg(target_arch = "wasm32")]
pub mod client;

#[cfg(not(target_arch = "wasm32"))]
use pserve::server::{Event, ToClientEvent};

#[cfg(target_arch = "wasm32")]
use pserve::client::{Blah, BlahUpdate, DataType};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FullState {
    #[serde(rename_all = "camelCase")]
    CheckBox { check_boxes: Vec<bool> },
    #[serde(rename_all = "camelCase")]
    MemeList { memes: Vec<String> },
}

impl FullState {
    pub fn from_name(state: &State, name: String) -> Self {
        match name.as_str() {
            "checkBoxes" => FullState::CheckBox {
                check_boxes: state.check_boxes.to_vec(),
            },
            "memeList" => FullState::MemeList {
                memes: state.meme_list.to_vec(),
            },
            _ => panic!("unknown full state name: {name}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StateUpdate {
    #[serde(rename_all = "camelCase")]
    CheckBox { check_boxes: Vec<(u32, bool)> },
    #[serde(rename_all = "camelCase")]
    MemeList { memes: Vec<(u32, String)> },
}

#[cfg(target_arch = "wasm32")]
impl BlahUpdate for StateUpdate {
    fn data_type(&self) -> DataType {
        match self {
            StateUpdate::CheckBox { check_boxes } => {
                DataType::Multiple(check_boxes.iter().map(|(id, _)| *id).collect())
            }
            StateUpdate::MemeList { memes } => {
                DataType::Multiple(memes.iter().map(|(id, _)| *id).collect())
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct CheckBoxStateEvent;

#[cfg(target_arch = "wasm32")]
impl pserve::client::Blah for CheckBoxStateEvent {
    type Full = FullState;
    type Update = StateUpdate;
    type EventData = Vec<bool>;

    fn name() -> String {
        "checkBoxes".to_string()
    }
    fn len(data: &Self::EventData) -> usize {
        data.len()
    }

    fn replace(&self, full: Self::Full, data: &mut Vec<bool>) {
        pserve::client::env::log("replacing all check boxes");
        if let FullState::CheckBox { mut check_boxes } = full {
            data.clear();
            data.append(&mut check_boxes);
        }
    }

    fn apply_update(&self, update: Self::Update, data: &mut Vec<bool>) {
        if let StateUpdate::CheckBox { check_boxes } = update {
            for (id, value) in check_boxes {
                match data.get_mut(id as usize) {
                    Some(v) => *v = value,
                    None => {
                        data.resize(id as usize + 1, false);
                        data[id as usize] = value;
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct MemeListStateEvent;

#[cfg(target_arch = "wasm32")]
impl pserve::client::Blah for MemeListStateEvent {
    type Full = FullState;
    type Update = StateUpdate;
    type EventData = Vec<String>;

    fn name() -> String {
        "memeList".to_string()
    }
    fn len(data: &Self::EventData) -> usize {
        data.len()
    }

    fn replace(&self, full: Self::Full, data: &mut Self::EventData) {
        pserve::client::env::log("replacing all memes");
        if let FullState::MemeList { mut memes } = full {
            data.clear();
            data.append(&mut memes);
        }
    }

    fn apply_update(&self, update: Self::Update, data: &mut Self::EventData) {
        if let StateUpdate::MemeList { memes } = update {
            for (id, value) in memes {
                match data.get_mut(id as usize) {
                    Some(v) => *v = value,
                    None => {
                        data.resize(id as usize + 1, "".to_string());
                        data[id as usize] = value;
                    }
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn request_full_state(state: &mut State, who: SocketAddr, name: String) -> Option<Event> {
    Some(Event::ToSpecificClient {
        who,
        event: ToClientEvent::Custom {
            event: serde_json::to_value(FullState::from_name(state, name)).unwrap(),
        },
    })
}

#[cfg(not(target_arch = "wasm32"))]
// TODO: #[processor]
pub fn render_component_for_everyone(
    _: &mut State,
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

#[cfg(not(target_arch = "wasm32"))]
pub fn toggle_check_box(state: &mut State, value: serde_json::Value) -> Option<Event> {
    pserve::server::tracing::info!("toggle_check_box: {:?}", value);
    let event: ClientEvent = serde_json::from_value(value).unwrap();

    let ClientEvent::ToggleCheckBox { id } = event else {
        return None;
    };

    state.check_boxes[id as usize] = !state.check_boxes[id as usize];

    Some(Event::ToAllClients(ToClientEvent::Custom {
        event: serde_json::to_value(StateUpdate::CheckBox {
            check_boxes: vec![(id, state.check_boxes[id as usize])],
        })
        .unwrap(),
    }))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn add_meme(state: &mut State, value: serde_json::Value) -> Option<Event> {
    pserve::server::tracing::info!("add_meme: {:?}", value);
    let event: ClientEvent = serde_json::from_value(value).unwrap();

    let ClientEvent::AddMeme { meme } = event else {
        return None;
    };

    state.meme_list.push(meme.clone());

    Some(Event::ToAllClients(ToClientEvent::Custom {
        event: serde_json::to_value(StateUpdate::MemeList {
            memes: vec![(state.meme_list.len() as u32 - 1, meme)],
        })
        .unwrap(),
    }))
}
