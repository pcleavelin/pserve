#[cfg(target_arch = "wasm32")]
use crate::client::{PERSISTENT_VALUES, current_dom_id};

use std::{collections::HashMap, hash::Hash};

#[derive(Clone)]
pub struct Signal<T: Clone, K: Clone + Hash + Eq> {
    // TODO: uh, don't use a raw pointer (maybe just a Cell would be fine?)
    pub(crate) inner: *mut SignalData<T, K>,
}

pub struct SignalData<T: Clone, K: Clone + Hash + Eq> {
    value: T,
    pub(crate) registered_dom_nodes: Vec<u32>,
    pub(crate) registered_dom_nodes_by_key: HashMap<K, u32>,
}

impl<T: Clone, K: Clone + Hash + Eq> SignalData<T, K> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            registered_dom_nodes: Vec::new(),
            registered_dom_nodes_by_key: HashMap::new(),
        }
    }
}

// NOTE: HAHAHAHA
impl<T: Clone, K: Clone + Hash + Eq> Copy for Signal<T, K> {}

impl<T: Clone, K: Clone + Hash + Eq> Signal<T, K> {
    #[cfg(target_arch = "wasm32")]
    pub fn reset(&mut self) {
        // FIXME: yolo
        unsafe {
            (*self.inner).registered_dom_nodes.clear();
            (*self.inner).registered_dom_nodes_by_key.clear();
        }
    }

    pub(crate) fn get_mut(&mut self) -> &mut T {
        // FIXME: yolo
        unsafe { &mut (*self.inner).value }
    }

    pub fn get(&self) -> T {
        #[cfg(target_arch = "wasm32")]
        {
            let dom_id = current_dom_id();

            if dom_id > 0 {
                // FIXME: yolo
                unsafe {
                    (*self.inner).registered_dom_nodes.push(dom_id);
                    (*self.inner).registered_dom_nodes.sort_unstable();
                    (*self.inner).registered_dom_nodes.dedup();
                }
            }
        }

        // FIXME: yolo
        unsafe { (*self.inner).value.clone() }
    }

    pub fn get_with_key(&self, index: K) -> T {
        #[cfg(target_arch = "wasm32")]
        {
            let dom_id = current_dom_id();

            if dom_id > 0 {
                // FIXME: yolo
                unsafe {
                    (*self.inner)
                        .registered_dom_nodes_by_key
                        .insert(index, dom_id);
                }
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
        #[cfg(target_arch = "wasm32")]
        unsafe {
            for dom_id in (*self.inner).registered_dom_nodes.iter().cloned() {
                PERSISTENT_VALUES.to_re_render.borrow_mut().insert(dom_id);
            }

            for (_, dom_id) in (*self.inner).registered_dom_nodes_by_key.iter() {
                PERSISTENT_VALUES.to_re_render.borrow_mut().insert(*dom_id);
            }
        }
    }
}
