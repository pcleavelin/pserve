use pserve::client::{Signal, use_signal};
use pserve::dom::*;

use crate::ClientEvent;

macro_rules! component_handler {
    ($($name:literal => $component:ident),* $(,)?) => {
        fn render_component(msg: &str) -> Option<String> {
            pserve::client::env::log(&msg);

            match msg {
                $($name => {
                    let built = $component().build(
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

// TODO: figure out away to not have to make users create this function themselves
#[unsafe(no_mangle)]
extern "C" fn js_render_component(fn_name: *mut u8, len: usize) -> *const u8 {
    let fn_name = unsafe { String::from_raw_parts(fn_name, len, len) };

    match render_component(&fn_name) {
        Some(html) => unsafe {
            pserve::client::RENDER_RESULT = pserve::client::RenderResult::from(html);
            &raw const pserve::client::RENDER_RESULT as *const _ as *const u8
        },
        None => std::ptr::null(),
    }
}

component_handler! {
    "home_page" => home_page,
    "meme_list" => meme_list,
    "server_communicator" => server_communicator,
}

fn server_communicator() -> DomNodeBuilder {
    let input = use_signal(|| "".to_string());

    DomNodeBuilder::default()
        .push("p", || {
            "Force load a component to every one on this website".into()
        })
        .push("select", || {
            DomNodeBuilder::default()
                .push("option", || "home_page".into())
                .push("option", || "meme_list".into())
                .push("option", || "server_communicator".into())
        })
        .on_input(move |value| input.set(value.to_string()))
        .push("button", || "Send to EVERYBODY".into())
        .on_click(move |_| {
            let msg = ClientEvent::RenderComponent {
                component_name: input.get(),
                dom_id: None,
            };

            pserve::client::env::send_event_to_server(&msg).unwrap();
        })
}

fn home_page() -> DomNodeBuilder {
    let my_strong_input = use_signal(|| "Hello, world!".to_string());
    let hello = use_signal(|| "Hello, I'm different".to_string());
    let show_meme_list = use_signal(|| false);

    DomNodeBuilder::default()
        .push("div", || server_communicator())
        .push("strong", || "List of things".into())
        .push("ul", move || {
            let mut n = DomNodeBuilder::default();

            for _ in 0..10 {
                n = n.push("li", move || {
                    DomNodeBuilder::default()
                        .push("a", move || {
                            format!("Link {}", my_strong_input.get().as_str()).into()
                        })
                        .attr("href", "/meme_list")
                });
            }

            n.push("h3", move || hello.get().as_str().into())
        })
        .push("div", move || {
            let mut n = DomNodeBuilder::default();

            n = n
                .push("button", move || {
                    let show_meme_list = show_meme_list.get();

                    if show_meme_list {
                        "Hide meme list".into()
                    } else {
                        "Show meme list".into()
                    }
                })
                .on_click(move |_| show_meme_list.set(!show_meme_list.get()));

            if show_meme_list.get() {
                n = n.push("span", move || meme_list())
            }

            n
        })
        .push("strong", move || my_strong_input.get().as_str().into())
        .push("input", || "".into())
        .on_input(move |value| my_strong_input.set(value.to_string()))
        .push("input", || "".into())
        .on_input(move |value| hello.set(value.to_string()))
}

fn meme_list() -> DomNodeBuilder {
    let memes: Signal<Vec<String>> = use_signal(|| {
        vec![
            "React".to_string(),
            "Rust".to_string(),
            "Dioxus".to_string(),
            "Leptos".to_string(),
        ]
    });
    let meme_entry = use_signal(|| "".to_string());

    DomNodeBuilder::default()
        .push("p", || "Meme list".into())
        .push("ul", move || {
            let mut n = DomNodeBuilder::default();

            let memes = memes.get();
            for meme in memes.into_iter() {
                n = n.push("li", move || {
                    DomNodeBuilder::default().push("p", {
                        let meme = meme.clone();
                        move || meme.as_str().into()
                    })
                });
            }

            n
        })
        .push("input", || "".into())
        .on_input(move |value| meme_entry.set(value.to_string()))
        .push("button", || "Add meme".into())
        .on_click(move |_| {
            let new_meme: String = meme_entry.get().clone();
            let mut more_memes = memes.get();
            more_memes.push(new_meme);
            memes.set(more_memes);

            // TODO: support inline pushing
            // memes.get_mut().push(new_meme);
            meme_entry.set("".to_string());
        })
}
