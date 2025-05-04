use pserve::client::{Signal, StateEvent, use_signal, use_state_event};
use pserve::dom::*;

use crate::{CheckBoxStateEvent, ClientEvent, MemeListStateEvent, NUMBER_OF_CHECKBOXES};

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
    "checkboxes" => checkboxes,
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
    let memes = use_state_event(MemeListStateEvent);
    let meme_entry = use_signal(|| "".to_string());

    DomNodeBuilder::default()
        .push("p", || "Meme list".into())
        .push("ul", move || {
            let mut n = DomNodeBuilder::default();

            for i in 0..memes.len() {
                n = n.push("li", move || {
                    DomNodeBuilder::default().push("p", move || {
                        let memes = memes.get_with_index(i as u32);
                        memes.get(i).unwrap().into()
                    })
                });
            }

            n
        })
        .push("input", || "".into())
        .on_input(move |value| meme_entry.set(value.to_string()))
        .push("button", || "Add meme".into())
        .on_click(move |_| {
            let new_meme = meme_entry.get();
            pserve::client::env::send_event_to_server(&ClientEvent::AddMeme { meme: new_meme })
                .unwrap();

            // TODO: support inline pushing
            // memes.get_mut().push(new_meme);
            meme_entry.set("".to_string());
        })
}

fn checkboxes() -> DomNodeBuilder {
    let check_boxes = use_state_event(CheckBoxStateEvent);

    DomNodeBuilder::default().push("div", move || {
        let mut n = DomNodeBuilder::default();

        n = n.push("p", || "A lotta Checkboxes".into());

        for i in 0..NUMBER_OF_CHECKBOXES {
            let check_boxes = check_boxes.clone();

            n = n.push("div", move || {
                let mut n = DomNodeBuilder::default();

                for j in 0..NUMBER_OF_CHECKBOXES {
                    let check_boxes = check_boxes.clone();
                    n = n.push("span", move || {
                        let mut n = DomNodeBuilder::default()
                            .push("input", || "".into())
                            .attr("type", "checkbox")
                            .on_click(move |_| {
                                pserve::client::env::log(&format!(
                                    "click {}",
                                    i * NUMBER_OF_CHECKBOXES + j
                                ));
                                let msg = ClientEvent::ToggleCheckBox {
                                    id: (i * NUMBER_OF_CHECKBOXES + j) as u32,
                                };

                                pserve::client::env::send_event_to_server(&msg).unwrap();
                            });

                        let check_boxes =
                            check_boxes.get_with_index((i * NUMBER_OF_CHECKBOXES + j) as u32);

                        if check_boxes
                            .get(i * NUMBER_OF_CHECKBOXES + j)
                            .is_some_and(|b| *b)
                        {
                            n = n.attr("checked", "");
                        }

                        n
                    });
                }

                n
            });
        }

        n
    })
}
