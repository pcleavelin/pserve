#[cfg(target_arch = "wasm32")]
pub mod client;

#[cfg(target_arch = "wasm32")]
use pserve::client::CookieEvent;

#[cfg(not(target_arch = "wasm32"))]
use pserve::server::{Event, ToClientEvent, tokio};
#[cfg(not(target_arch = "wasm32"))]
use std::net::SocketAddr;

use pserve::state::{IsSingleValue, Stateful, Valuable};

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
    DiscordLogin { code: String },
}

#[derive(Clone, Copy)]
pub struct UserInfoStateEvent;
impl Valuable<IsSingleValue> for UserInfoStateEvent {}
impl Stateful for UserInfoStateEvent {
    type Data = Option<DiscordUser>;
    type Key = ();

    fn name() -> &'static str {
        "user_info"
    }
}

#[cfg(target_arch = "wasm32")]
impl CookieEvent for UserInfoStateEvent {
    fn cookie_name() -> &'static str {
        "userInfo"
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
pub fn cookie_processor(
    state: &mut State,
    who: SocketAddr,
    name: String,
    value: String,
) -> Option<Event> {
    if name == "userInfo" {
        let user: Option<DiscordUser> = serde_json::from_str(&value).unwrap();

        if let Some(user) = user {
            pserve::server::tracing::info!("got cookie {name}: {value:?}, now logging them out");

            // NOTE: this is where you would check your DB if the cookie is valid
            state.connection_auth.insert(who, user.clone());

            // Some(Event::ToSpecificClient {
            //     who,
            //     event: ToClientEvent::Custom {
            //         event: serde_json::to_value(StateUpdate::UserInfo { user: None }).unwrap(),
            //     },
            // })
            None
        } else {
            None
        }
    } else {
        None
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn discord_login(
    state: &mut State,
    who: SocketAddr,
    value: serde_json::Value,
) -> Option<Event> {
    let event: ClientEvent = serde_json::from_value(value).unwrap();
    let ClientEvent::DiscordLogin { code } = event;

    let mut data = HashMap::new();

    let redirect_uri = format!("{}/auth", dotenv!("APP_ORIGIN"));
    data.insert("client_id", dotenv!("DISCORD_CLIENT_ID"));
    data.insert("client_secret", dotenv!("DISCORD_CLIENT_SECRET"));
    data.insert("grant_type", "authorization_code");
    data.insert("code", &code);
    data.insert("redirect_uri", &redirect_uri);

    let user: Result<_, Box<dyn std::error::Error>> = tokio::task::block_in_place(move || {
        tokio::runtime::Handle::current().block_on(async move {
            let client = reqwest::Client::new();

            let text = client
                .post("https://discord.com/api/oauth2/token")
                .form(&data)
                .send()
                .await?
                .text()
                .await?;

            let auth: Discord = serde_json::from_str(&text).inspect_err(|err| {
                pserve::server::tracing::error!(?text, "error logging in: {err:?}");
            })?;

            let text = client
                .get("https://discord.com/api/v10/users/@me")
                .bearer_auth(&auth.access_token)
                .send()
                .await?
                .text()
                .await?;

            let user: DiscordUser = serde_json::from_str(&text).inspect_err(|err| {
                pserve::server::tracing::error!(?text, "error getting user: {err:?}");
            })?;

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
                event: UserInfoStateEvent::as_single_update(Some(user)),
                //     ToClientEvent::Custom {
                //     event: serde_json::to_value(StateUpdate::UserInfo { user: Some(user) })
                //         .unwrap(),
                // },
            })
        }
        Err(e) => {
            pserve::server::tracing::error!("error logging in: {e:?}");
            return None;
        }
    }
}
