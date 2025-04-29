use crate::dom::{DomNodeBuilt, DomNodeBuiltBody, DomNodeUnbuilt, DomNodeUnbuiltBody};
use core::{
    any::Any,
    cell::{LazyCell, Ref, RefCell, RefMut},
    panic::Location,
    sync::atomic::{AtomicU32, Ordering},
};
use std::collections::{HashMap, HashSet};

pub mod env {
    use serde::Serialize;

    mod env_js {
        #[link(wasm_import_module = "Env")]
        unsafe extern "C" {
            pub fn alert(msg: *const u8, len: i32);
            pub fn log(msg: *const u8, len: i32);

            pub fn update_dom(dom_id: u32, html: *const u8, len: i32);

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
        env::log(&html);
        env::update_dom(dom_id, &html);
    }
}

pub struct PersistentState {
    cell: LazyCell<RefCell<HashMap<Location<'static>, Box<dyn Any>>>>,
    builders: LazyCell<RefCell<HashMap<u32, DomNodeUnbuilt>>>,
    built_nodes: LazyCell<RefCell<HashMap<u32, DomNodeBuilt>>>,
    to_re_render: LazyCell<RefCell<HashSet<u32>>>,
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

#[derive(Clone)]
pub struct Signal<T: Clone> {
    // TODO: uh, don't use a raw pointer (maybe just a Cell would be fine?)
    inner: *mut SignalData<T>,
}

pub struct SignalData<T: Clone> {
    value: T,
    registered_dom_nodes: Vec<u32>,
}

impl<T: Clone> SignalData<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            registered_dom_nodes: Vec::new(),
        }
    }
}

// NOTE: HAHAHAHA
impl<T: Clone> Copy for Signal<T> {}

impl<T: Clone> Signal<T> {
    pub fn reset(&mut self) {
        // FIXME: yolo
        unsafe {
            (*self.inner).registered_dom_nodes.clear();
        }
    }

    pub fn get_mut(&mut self) -> &mut T {
        // FIXME: yolo
        unsafe { &mut (*self.inner).value }
    }

    pub fn get(&self) -> T {
        let dom_id = current_dom_id();

        if dom_id > 0 {
            // FIXME: yolo
            unsafe {
                (*self.inner).registered_dom_nodes.push(dom_id);
                (*self.inner).registered_dom_nodes.sort_unstable();
                (*self.inner).registered_dom_nodes.dedup();
            }
        }

        // FIXME: yolo
        unsafe { (*self.inner).value.clone() }
    }

    #[track_caller]
    pub fn set(&self, value: T) {
        // FIXME: yolo
        unsafe {
            (*self.inner).value = value;
        }

        // FIXME: yolo
        unsafe {
            for dom_id in (*self.inner).registered_dom_nodes.iter().cloned() {
                PERSISTENT_VALUES.to_re_render.borrow_mut().insert(dom_id);
            }
        }
    }
}

#[track_caller]
pub fn use_signal<T: Clone + 'static>(f: impl FnOnce() -> T) -> Signal<T> {
    let location = Location::caller();

    let mut signal = persist_value(
        || Signal {
            inner: Box::into_raw(Box::new(SignalData::new(f()))),
        },
        *location,
    );

    signal.reset();

    signal
}

// NOTE: This is WASM so its probably fine lol
unsafe impl Send for PersistentState {}
unsafe impl Sync for PersistentState {}

pub static NEXT_DOM_ID: AtomicU32 = AtomicU32::new(1);
pub static CURRENT_SCOPE_DOM_ID: AtomicU32 = AtomicU32::new(0);

pub static PERSISTENT_VALUES: PersistentState = PersistentState {
    cell: LazyCell::new(|| RefCell::new(HashMap::new())),
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
