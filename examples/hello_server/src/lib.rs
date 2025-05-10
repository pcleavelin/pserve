#[cfg(target_arch = "wasm32")]
pub mod client;

#[cfg(not(target_arch = "wasm32"))]
use pserve::server::{Event, ToClientEvent};

use pserve::state::{Stateful, Valuable};

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

impl pserve::state::Stateful for MySuperCoolSingleValueStateEvent {
    // type Full = FullState;
    // type Update = StateUpdate;
    type Data = String;

    fn name() -> &'static str {
        "mySuperCoolSingleValueStateEvent"
    }
    // fn len(data: &Self::EventData) -> usize {
    //     data.len()
    // }

    // fn replace(full: Self::Full, data: &mut Self::EventData) {
    // pserve::client::env::log("replacing all check boxes");
    // if let FullState::CheckBox { mut check_boxes } = full {
    //     data.clear();
    //     data.append(&mut check_boxes);
    // }
    // }
}
impl pserve::state::Valuable<pserve::state::IsSingleValue> for MySuperCoolSingleValueStateEvent {}

#[derive(Clone, Copy)]
pub struct CheckBoxStateEvent;

impl pserve::state::Stateful for CheckBoxStateEvent {
    // type Full = FullState;
    // type Update = StateUpdate;
    type Data = Vec<bool>;

    fn name() -> &'static str {
        "checkBoxes"
    }
    // fn len(data: &Self::EventData) -> usize {
    //     data.len()
    // }

    // fn replace(full: Self::Full, data: &mut Self::EventData) {
    // pserve::client::env::log("replacing all check boxes");
    // if let FullState::CheckBox { mut check_boxes } = full {
    //     data.clear();
    //     data.append(&mut check_boxes);
    // }
    // }
}

// {
//     fn apply_update(update: Self::Update, data: &mut Vec<bool>) {
//         if let StateUpdate::CheckBox { check_boxes } = update {
//             for (id, value) in check_boxes {
//                 match data.get_mut(id as usize) {
//                     Some(v) => *v = value,
//                     None => {
//                         data.resize(id as usize + 1, false);
//                         data[id as usize] = value;
//                     }
//                 }
//             }
//         }
//     }
// }

#[derive(Clone, Copy)]
pub struct MemeListStateEvent;

impl pserve::state::Stateful for MemeListStateEvent {
    // type Full = FullState;
    // type Update = StateUpdate;
    type Data = Vec<String>;

    fn name() -> &'static str {
        "memeList"
    }
    // fn len(data: &Self::EventData) -> usize {
    //     data.len()
    // }

    // fn replace(full: Self::Full, data: &mut Self::EventData) {
    //     pserve::client::env::log("replacing all memes");
    //     if let FullState::MemeList { mut memes } = full {
    //         data.clear();
    //         data.append(&mut memes);
    //     }
    // }
}

// {
//     fn apply_update(update: Self::Update, data: &mut Self::EventData) {
//         if let StateUpdate::MemeList { memes } = update {
//             for (id, value) in memes {
//                 match data.get_mut(id as usize) {
//                     Some(v) => *v = value,
//                     None => {
//                         data.resize(id as usize + 1, "".to_string());
//                         data[id as usize] = value;
//                     }
//                 }
//             }
//         }
//     }
// }

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
            event: ToClientEvent::Custom {
                event: serde_json::to_value(state.meme_list.clone()).unwrap(),
            },
        }),
        "checkBoxes" => Some(Event::ToSpecificClient {
            who,
            event: ToClientEvent::Custom {
                event: serde_json::to_value(state.check_boxes.to_vec()).unwrap(),
            },
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

    None

    // Some(Event::ToAllClients(ToClientEvent::Custom {
    //     event: todo!(),
    //     // event: serde_json::to_value(StateUpdate::CheckBox {
    //     //     check_boxes: vec![(id, state.check_boxes[id as usize])],
    //     // })
    //     // .unwrap(),
    // }))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn add_meme(state: &mut State, _who: SocketAddr, value: serde_json::Value) -> Option<Event> {
    pserve::server::tracing::info!("add_meme: {:?}", value);
    let event: ClientEvent = serde_json::from_value(value).unwrap();

    let ClientEvent::AddMeme { meme } = event else {
        return None;
    };

    state.meme_list.push(meme.clone());

    None

    // Some(Event::ToAllClients(ToClientEvent::Custom {
    //     event: todo!(),
    //     // event: serde_json::to_value(StateUpdate::MemeList {
    //     //     memes: vec![(state.meme_list.len() as u32 - 1, meme)],
    //     // })
    //     // .unwrap(),
    // }))
}
