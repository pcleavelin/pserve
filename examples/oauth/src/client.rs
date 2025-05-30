use dotenvy_macro::dotenv;
use pserve::client::{use_cookie, use_signal};
use pserve::dom::DomNodeBuilder;

use crate::{ClientEvent, UserInfoStateEvent};

macro_rules! component_handler {
    ($($name:literal => $component:ident),* $(,)?) => {
        // TODO: figure out away to not have to make users create this function themselves
        #[unsafe(no_mangle)]
        extern "C" fn js_render_component(fn_name: *mut u8, fn_len: usize, params: *mut u8, params_len: usize) -> *const u8 {
            let fn_name = unsafe { String::from_raw_parts(fn_name, fn_len, fn_len) };
            let params = unsafe { String::from_raw_parts(params, params_len, params_len) };

            match render_component(&fn_name, &params) {
                Some(html) => unsafe {
                    pserve::client::RENDER_RESULT = pserve::client::RenderResult::from(html);
                    &raw const pserve::client::RENDER_RESULT as *const _ as *const u8
                },
                None => std::ptr::null(),
            }
        }

        fn render_component(msg: &str, params: &str) -> Option<String> {
            pserve::client::env::log(&msg);

            let params = if params.is_empty() {
                None
            } else {
                Some(params.to_string())
            };

            match msg {
                $($name => {
                    let built = $component(params).build(
                        &mut pserve::client::PERSISTENT_VALUES.get_builders_mut(),
                        &mut pserve::client::PERSISTENT_VALUES.get_built_nodes_mut(),
                        true,
                    );

                    Some(pserve::client::render_multi(built))
                })*
                _ => {
                    pserve::client::env::log("unknown component");
                    None
                }
            }
        }
    };
}

component_handler! {
    "home_page" => home_page,
    "auth" => auth,
}

fn home_page(params: Option<String>) -> DomNodeBuilder {
    let user = use_cookie(UserInfoStateEvent);

    DomNodeBuilder::default().push("div", move || {
        if let Some(user) = user.get() {
            DomNodeBuilder::default().push("p", move || format!("Hello {}!", user.username).into())
        } else {
            let authorize_uri = format!(
                "https://discord.com/api/oauth2/authorize?client_id={}&redirect_uri={}/auth&response_type=code&scope=guilds.members.read+guilds+identify",
                dotenv!("DISCORD_CLIENT_ID"),
                dotenv!("APP_ORIGIN")
            );

            DomNodeBuilder::default()
                .push("a", || "Login".into())
                .attr("href", &authorize_uri)
        }
    })
}

fn auth(params: Option<String>) -> DomNodeBuilder {
    let user = use_cookie(UserInfoStateEvent);
    let params = use_signal(|| params.map(|p| p[1..].split('=').nth(1).unwrap().to_string()));

    DomNodeBuilder::default().push("div", move || {
        if let Some(user) = user.get() {
            home_page(None)
        } else {
            if let Some(code) = params.get() {
                params.set(None);

                pserve::client::env::send_event_to_server(&ClientEvent::DiscordLogin {
                    code: code.to_string(),
                })
                .unwrap();

                DomNodeBuilder::default().push("p", || "Logging in...".into())
            } else {
                DomNodeBuilder::default().push("div", move || {
                    DomNodeBuilder::default().push("p", || "No code provided".into())
                })
            }
        }
    })
}
