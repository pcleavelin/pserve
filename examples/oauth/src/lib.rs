#[cfg(target_arch = "wasm32")]
pub mod client;

#[cfg(not(target_arch = "wasm32"))]
use pserve::server::{Event, ToClientEvent, tokio};
#[cfg(not(target_arch = "wasm32"))]
use std::net::SocketAddr;

#[cfg(target_arch = "wasm32")]
use pserve::client::{DataType, StateDataType, Stateful};

use dotenvy_macro::dotenv;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
pub struct State {
    pub connection_auth: HashMap<SocketAddr, DiscordUser>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientEvent {
    DiscordLogin{ code: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StateUpdate {
    UserInfo { user: DiscordUser },
}

#[cfg(target_arch = "wasm32")]
impl StateDataType for StateUpdate {
    fn data_type(&self) -> DataType {
        DataType::Single
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FullState {
    UserInfo { user_info: DiscordUser },
}

#[derive(Clone, Copy)]
pub struct UserInfoStateEvent;

#[cfg(target_arch = "wasm32")]
impl pserve::client::CookieEvent for UserInfoStateEvent {
    fn cookie_name() -> &'static str {
        "userInfo"
    }
}

#[cfg(target_arch = "wasm32")]
impl pserve::client::Stateful for UserInfoStateEvent {
    type Full = FullState;
    type Update = StateUpdate;
    type EventData = DiscordUser;

    fn name() -> String {
        "user_info".to_string()
    }
    fn len(data: &Self::EventData) -> usize {
        1
    }

    fn replace(full: Self::Full, data: &mut DiscordUser) {
        pserve::client::env::log("replacing user info");
        if let FullState::UserInfo { mut user_info } = full {
            *data = user_info;
        }
    }

    fn apply_update(update: Self::Update, data: &mut DiscordUser) {
        if let StateUpdate::UserInfo { user } = update {
            *data = user;
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Discord {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: usize,
    pub refresh_token: String,
    pub scope: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct DiscordUser {
    pub username: String,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn discord_login(state: &mut State, who: SocketAddr, value: serde_json::Value) -> Option<Event> {
    let event: ClientEvent = serde_json::from_value(value).unwrap();
    let ClientEvent::DiscordLogin{ code } = event;

    let mut data = HashMap::new();

    let redirect_uri = format!("{}/auth", dotenv!("APP_ORIGIN"));
    data.insert("client_id", dotenv!("DISCORD_CLIENT_ID"));
    data.insert("client_secret", dotenv!("DISCORD_CLIENT_SECRET"));
    data.insert("grant_type", "authorization_code");
    data.insert("code", &code);
    data.insert("redirect_uri", &redirect_uri);

    let user: Result<_, reqwest::Error> = tokio::task::block_in_place(move || { 
        tokio::runtime::Handle::current().block_on(async move {
            let client = reqwest::Client::new();

            let auth: Discord = client
                .post("https://discord.com/api/oauth2/token")
                .form(&data)
                .send().await?
                .json().await?;

            let user: DiscordUser = client
                .get("https://discord.com/api/v10/users/@me")
                .bearer_auth(&auth.access_token)
                .send().await?
                .json().await?;

            Ok(user)
        })
    });

    match user {
        Ok(user) => {
            pserve::server::tracing::info!("logged in as {user:?}");

            // FIXME: currently no way to remove clients who have disconnected
            state.connection_auth.insert(who, user.clone());

            Some(Event::ToSpecificClient {
                who,
                event: ToClientEvent::Custom {
                    event: serde_json::to_value(StateUpdate::UserInfo {
                        user
                    }).unwrap(),
                },
            })
        }
        Err(e) => {
            pserve::server::tracing::error!("error logging in: {e:?}");
            return None;
        }
    }
}
