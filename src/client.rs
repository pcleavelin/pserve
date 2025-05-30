extern crate alloc;

use crate::dom::{DomNodeBuilt, DomNodeBuiltBody, DomNodeUnbuilt, DomNodeUnbuiltBody};
use crate::signal::{Signal, SignalData};
use crate::state::{SettableEvent, StateEvent, StateInner, Stateful, Valuable};
use core::{
    any::Any,
    cell::{LazyCell, Ref, RefCell, RefMut},
    panic::Location,
    sync::atomic::{AtomicU32, Ordering},
};
use serde::de::DeserializeOwned;
use std::any::TypeId;
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;

pub mod env {
    use serde::Serialize;

    mod env_js {
        #[link(wasm_import_module = "Env")]
        unsafe extern "C" {
            pub fn alert(msg: *const u8, len: i32);
            pub fn log(msg: *const u8, len: i32);

            pub fn update_dom(dom_id: u32, html: *const u8, len: i32);
            pub fn update_cookie(msg: *const u8, len: i32);
            pub fn get_cookie(msg: *const u8, len: i32, cookie_len: *mut i32) -> *const u8;

            pub fn send_event_to_server(msg: *const u8, len: i32);
        }
    }

    pub fn alert(msg: &str) {
        unsafe { env_js::alert(msg.as_ptr(), msg.len() as i32) }
    }
    pub fn log(msg: &str) {
        unsafe { env_js::log(msg.as_ptr(), msg.len() as i32) }
    }
    pub fn update_dom(dom_id: u32, html: &str) {
        unsafe { env_js::update_dom(dom_id, html.as_ptr(), html.len() as i32) }
    }
    // FIXME: set global cookie for the whole domain (instead of just the current path)
    pub fn update_cookie(name: &str, value: impl Serialize) {
        let value = serde_json::to_string(&value).unwrap();
        let cookie = format!("{}={}", name, value);
        unsafe { env_js::update_cookie(cookie.as_ptr(), cookie.len() as i32) }
    }
    pub fn get_cookie(name: &str) -> Option<String> {
        let mut len = 0;
        let cookie = unsafe { env_js::get_cookie(name.as_ptr(), name.len() as i32, &mut len) };
        if cookie.is_null() {
            return None;
        }

        let cookie =
            unsafe { String::from_raw_parts(cookie as *mut u8, len as usize, len as usize) };
        log(&format!("got cookie {cookie:?}"));
        Some(cookie)
    }
    pub fn send_event_to_server<T: Serialize>(msg: &T) -> Result<(), serde_json::Error> {
        let msg = serde_json::to_string(msg)?;
        unsafe { env_js::send_event_to_server(msg.as_ptr(), msg.len() as i32) }

        Ok(())
    }
}

// TODO: delete this dumb thing, OR AT LEAST make it a RefCell, _definitely_ causes memory corruption
pub static mut RENDER_RESULT: RenderResult = RenderResult {
    ptr: std::ptr::null(),
    len: 0,
};

#[repr(C)]
pub struct RenderResult {
    ptr: *const u8,
    len: i32,
}

impl From<String> for RenderResult {
    fn from(mut s: String) -> Self {
        s.shrink_to_fit();

        Self {
            ptr: s.as_ptr(),
            len: s.len() as i32,
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn alloc_string(len: i32) -> u32 {
    String::with_capacity(len as usize).leak().as_ptr() as u32
}

// TODO: don't actually send the function pointer between javascript and wasm
#[unsafe(no_mangle)]
extern "C" fn call_fn_ptr(value: *mut u8, len: i32, ptr: *const Box<dyn Fn(&str)>) {
    let value = unsafe { String::from_raw_parts(value, len as usize, len as usize) };
    let func = unsafe { &*(ptr as *const Box<dyn Fn(&str)>) };

    func(&value);
}

#[unsafe(no_mangle)]
extern "C" fn handle_custom_event(value: *mut u8, len: i32) {
    let value = unsafe { String::from_raw_parts(value, len as usize, len as usize) };
    let json_value: serde_json::Value = match serde_json::from_str(&value) {
        Ok(value) => value,
        Err(e) => {
            env::log(&format!("failed to deserialize custom event: {e}: {value}"));
            return;
        }
    };

    let mut event_subscriptions = PERSISTENT_VALUES.event_subscriptions.borrow_mut();
    for event in event_subscriptions.values_mut() {
        event.set(json_value.clone());
    }
}

#[unsafe(no_mangle)]
extern "C" fn rerender() {
    for dom_id in PERSISTENT_VALUES.to_re_render.borrow_mut().drain() {
        {
            let mut builders = crate::client::PERSISTENT_VALUES.get_builders_mut();
            let mut built_nodes = crate::client::PERSISTENT_VALUES.get_built_nodes_mut();

            // TODO: don't just duplicate what dom.rs does
            if let Some(node) = builders.remove(&dom_id) {
                if let Some(body) = &node.body {
                    match &body {
                        DomNodeUnbuiltBody::Text(text) => {
                            built_nodes.insert(
                                dom_id,
                                DomNodeBuilt {
                                    id: dom_id,
                                    body: DomNodeBuiltBody::Text(text.clone()),
                                },
                            );
                        }
                        DomNodeUnbuiltBody::Constructor(ctor) => {
                            env::log(&format!("ctor dom_id: {}", dom_id));
                            crate::client::set_current_dom_id(dom_id);

                            if let Some(prev_built_nodes) = built_nodes.remove(&dom_id) {
                                if let DomNodeBuiltBody::Nodes(prev_child_body) =
                                    prev_built_nodes.body
                                {
                                    for child_id in prev_child_body {
                                        built_nodes.remove(&child_id);
                                        builders.remove(&child_id);
                                    }
                                }
                            }

                            // TODO: don't just giga-increment the NEXT_DOM_ID on every re-render
                            let builder = ctor();
                            let child_body = builder.build(&mut builders, &mut built_nodes, true);

                            crate::client::set_current_dom_id(0);

                            built_nodes.insert(
                                dom_id,
                                DomNodeBuilt {
                                    id: dom_id,
                                    body: DomNodeBuiltBody::Nodes(child_body),
                                },
                            );
                        }
                    }
                }

                builders.insert(dom_id, node);
            }
        }

        let html = render(dom_id);
        env::update_dom(dom_id, &html);
    }
}

pub struct PersistentState {
    cell: LazyCell<RefCell<HashMap<Location<'static>, Box<dyn Any>>>>,
    event_subscriptions: LazyCell<RefCell<HashMap<TypeId, Box<dyn SettableEvent>>>>,
    builders: LazyCell<RefCell<HashMap<u32, DomNodeUnbuilt>>>,
    built_nodes: LazyCell<RefCell<HashMap<u32, DomNodeBuilt>>>,
    pub(crate) to_re_render: LazyCell<RefCell<HashSet<u32>>>,
}

impl PersistentState {
    pub fn get_builders<'a>(&'a self) -> Ref<'a, HashMap<u32, DomNodeUnbuilt>> {
        self.builders.borrow()
    }
    pub fn get_builders_mut<'a>(&'a self) -> RefMut<'a, HashMap<u32, DomNodeUnbuilt>> {
        self.builders.borrow_mut()
    }

    pub fn get_built_nodes<'a>(&'a self) -> Ref<'a, HashMap<u32, DomNodeBuilt>> {
        self.built_nodes.borrow()
    }
    pub fn get_built_nodes_mut<'a>(&'a self) -> RefMut<'a, HashMap<u32, DomNodeBuilt>> {
        self.built_nodes.borrow_mut()
    }
}

#[track_caller]
pub fn use_signal<T: Clone + 'static>(f: impl FnOnce() -> T) -> Signal<T, ()> {
    let location = Location::caller();
    use_signal_with_caller(f, *location)
}

fn use_signal_with_caller<T: Clone + 'static>(
    f: impl FnOnce() -> T,
    location: Location<'static>,
) -> Signal<T, ()> {
    let mut signal = persist_value(
        || Signal {
            inner: Box::into_raw(Box::new(SignalData::new(f()))),
        },
        location,
    );

    signal.reset();

    signal
}

pub trait CookieEvent {
    fn cookie_name() -> &'static str;
}

// TODO: change `data` to be an enum of Single and Multiple instead of the crazy type shenanigans
// I'm doing above
// #[derive(Clone, Copy)]
// pub struct StateEvent<T: Stateful + Clone + 'static, M: Clone + 'static>
// where
//     <T as Stateful>::Data: DeserializeOwned + Default + Clone,
// {
//     data: Signal<StateInner<T, M>>,
// }

impl<T: Stateful + Clone, M: Clone + 'static> StateEvent<T, M>
where
    <T as Stateful>::Data: DeserializeOwned + Default + Clone,
{
    pub fn get(&self) -> <T as Stateful>::Data {
        self.data.get().inner
    }

    pub fn get_with_index(&self, index: T::Key) -> <T as Stateful>::Data {
        self.data.get_with_key(index).inner
    }

    // pub fn len(&self) -> usize {
    //     // TODO: use a non-cloning method
    //     T::len(&self.data.get().inner)
    // }

    pub fn on_update(mut self, f: fn(&<T as Stateful>::Data)) -> Self {
        self.data.get_mut().on_update = Some(f);
        self
    }
}

// impl<T: Stateful + Clone> SettableEvent for StateEvent<T>
// where
//     <T as Stateful>::Data: DeserializeOwned + Default + Clone,
// {
//     fn as_any(&self) -> &dyn Any {
//         self
//     }

// fn set(&mut self, value: serde_json::Value) {
//     if let Ok(full) = serde_json::from_value::<<T as Stateful>::Full>(value.clone()) {
//         env::log("got full state");
//
//         unsafe {
//             if let Ok(mut to_re_render) = PERSISTENT_VALUES.to_re_render.try_borrow_mut() {
//                 for dom_id in (*self.data.inner).registered_dom_nodes.iter().cloned() {
//                     to_re_render.insert(dom_id);
//                 }
//                 for dom_id in (*self.data.inner).registered_dom_nodes_by_key.values() {
//                     to_re_render.insert(*dom_id);
//                 }
//             }
//         }
//
//         T::replace(full, &mut self.data.get_mut().inner);
//
//         if let Some(on_update) = self.data.get().on_update {
//             on_update(&self.data.get().inner);
//         }
//     }
// }
// }

// else if let Ok(update) = serde_json::from_value::<<T as Stateful>::Update>(value.clone())
// {
//     env::log("got partial state");
//
//     if let Ok(mut to_re_render) = PERSISTENT_VALUES.to_re_render.try_borrow_mut() {
//         match update.data_type() {
//             DataType::Single => unsafe {
//                 for dom_id in (*self.data.inner).registered_dom_nodes.iter().cloned() {
//                     to_re_render.insert(dom_id);
//                 }
//             },
//             DataType::Multiple(keys) => unsafe {
//                 for dom_id in (*self.data.inner).registered_dom_nodes.iter().cloned() {
//                     to_re_render.insert(dom_id);
//                 }
//                 for (_, dom_id) in (*self.data.inner)
//                     .registered_dom_nodes_by_key
//                     .iter()
//                     .filter(|(key, _)| keys.contains(key))
//                 {
//                     to_re_render.insert(*dom_id);
//                 }
//             },
//         }
//     }
//
//     let mut data = self.data.get_mut();
//     T::apply_update(update, &mut data.inner);
//
//     if let Some(on_update) = data.on_update {
//         on_update(&data.inner);
//     } else {
//         env::log("no on_update");
//     }
// } else if let Ok(value) = serde_json::from_value::<<T as Stateful>::Data>(value) {
//     env::log("got exact state");
//
//     unsafe {
//         if let Ok(mut to_re_render) = PERSISTENT_VALUES.to_re_render.try_borrow_mut() {
//             for dom_id in (*self.data.inner).registered_dom_nodes.iter().cloned() {
//                 to_re_render.insert(dom_id);
//             }
//             for dom_id in (*self.data.inner).registered_dom_nodes_by_key.values() {
//                 to_re_render.insert(*dom_id);
//             }
//         }
//     }
//
//     let mut data = self.data.get_mut();
//     data.inner = value;
//
//     if let Some(on_update) = data.on_update {
//         on_update(&data.inner);
//     } else {
//         env::log("no on_update");
//     }
// }
//     }
// }

pub fn use_state_event<M: Clone + 'static, T: Stateful + Valuable<M> + Clone + 'static>(
    _: T,
) -> StateEvent<T, M>
where
    StateEvent<T, M>: SettableEvent,
    <T as Stateful>::Data: DeserializeOwned + Default + Clone,
{
    let mut event_subscriptions = PERSISTENT_VALUES.event_subscriptions.borrow_mut();

    if let Some(state_event) = event_subscriptions.get(&TypeId::of::<T>()) {
        let state_event = (*state_event)
            .as_any()
            .downcast_ref::<StateEvent<T, M>>()
            .unwrap();

        return state_event.clone();
    } else {
        let data = SignalData::new(StateInner {
            inner: <T as Stateful>::Data::default(),
            on_update: None,
            _marker: PhantomData,
        });
        let state_event = StateEvent {
            data: Signal {
                inner: Box::into_raw(Box::new(data)),
            },
        };

        event_subscriptions.insert(TypeId::of::<T>(), Box::new(state_event.clone()));

        // TODO: don't hide to server event behind non-wasm arch flag
        #[derive(serde::Serialize)]
        #[serde(tag = "type", rename_all = "camelCase")]
        enum Event {
            RequestFullState { name: String },
        }
        env::send_event_to_server(&Event::RequestFullState {
            name: T::name().to_string(),
        })
        .unwrap();

        state_event
    }
}

// pub fn use_state_event<T>(event: T)
// where
//     T: MultipleValueStateful<<T as Stateful>::Data> + Clone + 'static,
//     <T as Stateful>::Data:
//         GenericInnerType + GenericIndexedCollection + DeserializeOwned + Default + Clone,
// {
//     let mut event_subscriptions = PERSISTENT_VALUES.event_subscriptions.borrow_mut();
//
//     if let Some(state_event) = event_subscriptions.get(&TypeId::of::<T>()) {
//         let state_event = (*state_event)
//             .as_any()
//             .downcast_ref::<StateEvent<T>>()
//             .unwrap();
//
//         return state_event.clone();
//     } else {
//         let data = SignalData::new(StateInner {
//             inner: <T as Stateful>::Data::default(),
//             on_update: None,
//         });
//         let state_event = StateEvent {
//             data: Signal {
//                 inner: Box::into_raw(Box::new(data)),
//             },
//         };
//
//         event_subscriptions.insert(TypeId::of::<T>(), Box::new(state_event.clone()));
//
//         // TODO: don't hide to server event behind non-wasm arch flag
//         #[derive(serde::Serialize)]
//         #[serde(tag = "type", rename_all = "camelCase")]
//         enum Event {
//             RequestFullState { name: String },
//         }
//         env::send_event_to_server(&Event::RequestFullState { name: T::name() }).unwrap();
//
//         state_event
//     }
// }

// TODO: don't allow further `on_update` calls after this one (or allow chaining them?)
pub fn use_cookie<M: Clone + 'static, T: Stateful + Valuable<M> + CookieEvent + Clone + 'static>(
    event: T,
) -> StateEvent<T, M>
where
    StateEvent<T, M>: SettableEvent,
    <T as Stateful>::Data: DeserializeOwned + Default + Clone + std::fmt::Debug,
{
    // NOTE: this will send the `RequestFullState` event to the server
    let mut state_event = use_state_event(event).on_update(|value| {
        env::log(&format!(
            "updating cookie {} to {value:?}",
            T::cookie_name()
        ));

        // TODO: tell the server what cookie we have
        env::update_cookie(T::cookie_name(), value);

        // TODO: don't hide to server event behind non-wasm arch flag
        #[derive(serde::Serialize)]
        #[serde(tag = "type", rename_all = "camelCase")]
        enum Event {
            Cookie { name: String, value: String },
        }
        env::send_event_to_server(&Event::Cookie {
            name: T::cookie_name().to_string(),
            value: serde_json::to_string(&value).unwrap(),
        })
        .unwrap();
    });

    if let Some(cookie) = env::get_cookie(T::cookie_name()) {
        let value = serde_json::from_str(&cookie).unwrap();
        state_event.set(value);
    }

    state_event
}

// NOTE: This is WASM so its probably fine lol
unsafe impl Send for PersistentState {}
unsafe impl Sync for PersistentState {}

pub static NEXT_DOM_ID: AtomicU32 = AtomicU32::new(1);
pub static CURRENT_SCOPE_DOM_ID: AtomicU32 = AtomicU32::new(0);

pub static PERSISTENT_VALUES: PersistentState = PersistentState {
    cell: LazyCell::new(|| RefCell::new(HashMap::new())),
    event_subscriptions: LazyCell::new(|| RefCell::new(HashMap::new())),
    builders: LazyCell::new(|| RefCell::new(HashMap::new())),
    built_nodes: LazyCell::new(|| RefCell::new(HashMap::new())),
    to_re_render: LazyCell::new(|| RefCell::new(HashSet::new())),
};

pub fn persist_value<T: Clone + 'static>(f: impl FnOnce() -> T, location: Location<'static>) -> T {
    let mut values = PERSISTENT_VALUES.cell.borrow_mut();

    if let Some(value) = values.get(&location) {
        value.downcast_ref::<T>().unwrap().clone()
    } else {
        let value = f();
        values.insert(location, Box::new(value.clone()));
        value
    }
}

pub fn next_dom_id() -> u32 {
    NEXT_DOM_ID.fetch_add(1, Ordering::SeqCst)
}

pub fn current_dom_id() -> u32 {
    CURRENT_SCOPE_DOM_ID.load(Ordering::SeqCst)
}
pub fn set_current_dom_id(id: u32) {
    CURRENT_SCOPE_DOM_ID.store(id, Ordering::SeqCst);
}

pub fn render_multi(dom_ids: impl IntoIterator<Item = u32>) -> String {
    let mut string = String::new();

    for dom_id in dom_ids {
        string.push_str(&render(dom_id));
    }

    string
}

pub fn render(dom_id: u32) -> String {
    let mut string = String::new();

    let built_nodes = &crate::client::PERSISTENT_VALUES.get_built_nodes();
    let builders = &crate::client::PERSISTENT_VALUES.get_builders();

    if let (Some(built_node), Some(builder)) = (built_nodes.get(&dom_id), builders.get(&dom_id)) {
        {
            if !builder.tag.is_empty() {
                string.push_str(&format!("<{} data-pserve-id={}", builder.tag, dom_id));
            }

            for (attr, value) in &builder.attributes {
                if value.is_empty() {
                    string.push_str(&format!(" {attr} "));
                } else {
                    string.push_str(&format!(" {attr}='{value}' "));
                }
            }

            if let Some(on_input) = &builder.on_input {
                string.push_str(&format!(
                    " oninput=\"call_wasm_fn_ptr(this.value, {})\"",
                    on_input.as_ref() as *const Box<dyn Fn(&str)> as i32
                ));
            }
            if let Some(on_click) = &builder.on_click {
                string.push_str(&format!(
                    " onclick=\"call_wasm_fn_ptr(this.value, {})\"",
                    on_click.as_ref() as *const Box<dyn Fn(&str)> as i32
                ));
            }

            if !builder.tag.is_empty() {
                string.push('>');
            }
        }

        match &built_node.body {
            DomNodeBuiltBody::Text(text) => string.push_str(text),
            DomNodeBuiltBody::Nodes(nodes) => {
                for node_id in nodes {
                    string.push_str(&render(*node_id));
                }
            }
        }

        if !builder.tag.is_empty() {
            string.push_str(&format!("</{}>", builder.tag));
        }
    }

    string
}
