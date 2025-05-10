use std::{
    any::{Any, TypeId},
    cell::{LazyCell, RefCell},
    collections::HashMap,
    hash::Hash,
    marker::PhantomData,
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;

use crate::signal::Signal;

#[cfg(not(target_arch = "wasm32"))]
use crate::server::{Event, ToClientEvent};

#[cfg(target_arch = "wasm32")]
use crate::client::PERSISTENT_VALUES;

#[derive(Serialize, Deserialize)]
pub struct StatefulClientEvent<T: Stateful, D: Serialize> {
    pub(crate) state_key: String,
    pub(crate) event: D,

    #[serde(skip)]
    _stateful: PhantomData<T>,
}

pub trait Stateful: Sized {
    type Data: Serialize + DeserializeOwned + Default + Clone + 'static;
    type Key: DeserializeOwned + Clone + Hash + Eq;

    fn name() -> &'static str;

    #[cfg(not(target_arch = "wasm32"))]
    fn as_single_update(value: Self::Data) -> ToClientEvent {
        ToClientEvent::Custom {
            event: serde_json::to_value(StatefulClientEvent {
                state_key: Self::name().to_string(),
                event: value,
                _stateful: PhantomData::<Self>,
            })
            .unwrap(),
        }
    }
}

pub trait SettableEvent {
    fn as_any(&self) -> &dyn Any;
    fn set(&mut self, value: serde_json::Value);
}

pub trait InnerCollection {
    type Key: DeserializeOwned + Default + Clone + PartialEq;
    type Inner: DeserializeOwned + Default + Clone;

    fn len(&self) -> u32;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn set_at(&mut self, key: Self::Key, value: Self::Inner);
}

impl<T: DeserializeOwned + Default + Clone> InnerCollection for Vec<T> {
    type Key = u32;
    type Inner = T;

    fn len(&self) -> u32 {
        self.len() as u32
    }

    fn set_at(&mut self, key: Self::Key, value: Self::Inner) {
        // FIXME: do a bounds check and potentially re-allocate
        // (client side state might not have the same length as the server)
        // self[key as usize] = value;

        match self.get_mut(key as usize) {
            Some(v) => *v = value,
            None => {
                self.resize(key as usize + 1, T::default());
                self[key as usize] = value;
            }
        }
    }
}

impl<K, T> InnerCollection for HashMap<K, T>
where
    K: DeserializeOwned + Default + Clone + PartialEq + Eq + Hash,
    T: DeserializeOwned + Default + Clone,
{
    type Key = K;
    type Inner = T;

    fn len(&self) -> u32 {
        self.len() as u32
    }

    fn set_at(&mut self, key: Self::Key, value: Self::Inner) {
        self.insert(key, value);
    }
}

#[derive(Clone, Copy)]
pub struct StateEvent<T: Stateful + Clone + 'static, M: Clone>
where
    <T as Stateful>::Data: DeserializeOwned + Default + Clone,
{
    pub(crate) data: Signal<StateInner<T, M>, T::Key>,
}

type MultipleValueUpdateArray<T> =
    Vec<(<T as InnerCollection>::Key, <T as InnerCollection>::Inner)>;
pub trait MultipleValueUpdate
where
    Self: Stateful,
    Self::Data: InnerCollection,
    <Self::Data as InnerCollection>::Key: Serialize + DeserializeOwned,
    <Self::Data as InnerCollection>::Inner: Serialize + DeserializeOwned,
{
    fn apply_update(
        update: MultipleValueUpdateArray<Self::Data>,
        data: &mut Self::Data,
    ) -> Vec<<Self::Data as InnerCollection>::Key>;

    #[cfg(not(target_arch = "wasm32"))]
    fn as_full_update<'a>(
        data: impl IntoIterator<Item = &'a <Self::Data as InnerCollection>::Inner>,
    ) -> ToClientEvent {
        ToClientEvent::Custom {
            event: serde_json::to_value(StatefulClientEvent {
                state_key: Self::name().to_string(),
                event: data.into_iter().enumerate().collect::<Vec<(_, _)>>(),
                _stateful: PhantomData::<Self>,
            })
            .unwrap(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn as_update(
        key: <Self::Data as InnerCollection>::Key,
        value: <Self::Data as InnerCollection>::Inner,
    ) -> ToClientEvent {
        ToClientEvent::Custom {
            event: serde_json::to_value(StatefulClientEvent {
                state_key: Self::name().to_string(),
                event: vec![(key, value)],
                _stateful: PhantomData::<Self>,
            })
            .unwrap(),
        }
    }
}

impl<T> MultipleValueUpdate for T
where
    T: Stateful,
    T::Data: InnerCollection,
    <T::Data as InnerCollection>::Key: Serialize + DeserializeOwned,
    <T::Data as InnerCollection>::Inner: Serialize + DeserializeOwned,
{
    fn apply_update(
        update: MultipleValueUpdateArray<Self::Data>,
        data: &mut Self::Data,
    ) -> Vec<<Self::Data as InnerCollection>::Key> {
        let keys = update.iter().map(|(key, _)| key.clone()).collect();

        for (key, value) in update {
            data.set_at(key, value);
        }

        keys
    }
}

#[derive(Clone, Copy)]
pub struct IsSingleValue;
#[derive(Clone, Copy)]
pub struct IsMultipleValue;

pub trait InnerUpdate<T: Stateful> {
    fn apply_update(&mut self, update: serde_json::Value) -> Vec<T::Key>;
}
pub trait Valuable<T> {}

impl<T> Valuable<IsMultipleValue> for T
where
    T: Stateful + MultipleValueUpdate,
    T::Data: InnerCollection,
    <T::Data as InnerCollection>::Key: Serialize + DeserializeOwned,
    <T::Data as InnerCollection>::Inner: Serialize + DeserializeOwned,
{
}

#[derive(Clone, Copy)]
pub struct StateInner<T: Stateful + Clone + 'static, M> {
    pub(crate) inner: T::Data,
    pub(crate) on_update: Option<fn(&T::Data)>,
    pub(crate) _marker: PhantomData<M>,
}

impl<T: Stateful + Clone> InnerUpdate<T> for StateInner<T, IsSingleValue>
where
    <T as Stateful>::Data: DeserializeOwned + Default + Clone,
{
    fn apply_update(&mut self, update: serde_json::Value) -> Vec<T::Key> {
        #[cfg(target_arch = "wasm32")]
        crate::client::env::log(&format!("setting single value: {update:?}"));

        if let Ok(value) =
            serde_json::from_value::<StatefulClientEvent<T, <T as Stateful>::Data>>(update)
        {
            #[cfg(target_arch = "wasm32")]
            crate::client::env::log("successfully deserialized single value update");

            if value.state_key == T::name() {
                self.inner = value.event;
            } else {
                #[cfg(target_arch = "wasm32")]
                crate::client::env::log("couldn't deserialize single value update");
            }
        } else {
            #[cfg(target_arch = "wasm32")]
            crate::client::env::log("couldn't deserialize single value update");
        }

        Vec::new()
    }
}

impl<T: MultipleValueUpdate + Clone> InnerUpdate<T> for StateInner<T, IsMultipleValue>
where
    <T as Stateful>::Data: InnerCollection<Key = T::Key> + DeserializeOwned + Default + Clone,
    <T::Data as InnerCollection>::Key: Serialize + DeserializeOwned,
    <T::Data as InnerCollection>::Inner: Serialize + DeserializeOwned,
{
    fn apply_update(&mut self, update: serde_json::Value) -> Vec<T::Key> {
        // ) -> Vec<<T::Data as InnerCollection>::Key> {
        #[cfg(target_arch = "wasm32")]
        crate::client::env::log(&format!("setting multiple values: {update:?}"));

        if let Ok(value) = serde_json::from_value::<
            StatefulClientEvent<T, MultipleValueUpdateArray<T::Data>>,
        >(update)
        {
            #[cfg(target_arch = "wasm32")]
            crate::client::env::log("successfully deserialized multiple value update");

            if value.state_key == T::name() {
                T::apply_update(value.event, &mut self.inner)
            } else {
                Vec::new()
            }
        } else {
            #[cfg(target_arch = "wasm32")]
            crate::client::env::log("couldn't deserialize multiple value update");

            Vec::new()
        }
    }
}

impl<T: Stateful + Clone> SettableEvent for StateEvent<T, IsSingleValue>
where
    StateInner<T, IsSingleValue>: InnerUpdate<T>,
    <T as Stateful>::Data: DeserializeOwned + Default + Clone,
{
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn set(&mut self, value: serde_json::Value) {
        let data = self.data.get_mut();
        data.apply_update(value);

        #[cfg(target_arch = "wasm32")]
        {
            if let Ok(mut to_re_render) = PERSISTENT_VALUES.to_re_render.try_borrow_mut() {
                unsafe {
                    for dom_id in (*self.data.inner).registered_dom_nodes.iter().cloned() {
                        to_re_render.insert(dom_id);
                    }
                }
            }

            if let Some(on_update) = self.data.get().on_update {
                on_update(&self.data.get().inner);
            }
        }
    }
}

impl<T: Stateful + MultipleValueUpdate + Clone> SettableEvent for StateEvent<T, IsMultipleValue>
where
    StateInner<T, IsMultipleValue>: InnerUpdate<T>,
    <T as Stateful>::Data: InnerCollection + DeserializeOwned + Default + Clone,
    <T::Data as InnerCollection>::Key: Serialize + DeserializeOwned,
    <T::Data as InnerCollection>::Inner: Serialize + DeserializeOwned,
{
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn set(&mut self, value: serde_json::Value) {
        let data = self.data.get_mut();
        let keys = data.apply_update(value);

        #[cfg(target_arch = "wasm32")]
        {
            if let Ok(mut to_re_render) = PERSISTENT_VALUES.to_re_render.try_borrow_mut() {
                unsafe {
                    for dom_id in (*self.data.inner).registered_dom_nodes.iter().cloned() {
                        to_re_render.insert(dom_id);
                    }
                    for (_, dom_id) in (*self.data.inner)
                        .registered_dom_nodes_by_key
                        .iter()
                        .filter(|(key, _)| keys.contains(*key))
                    {
                        to_re_render.insert(*dom_id);
                    }
                }
            }

            if let Some(on_update) = self.data.get().on_update {
                on_update(&self.data.get().inner);
            }
        }
    }
}

const SUB: LazyCell<RefCell<HashMap<TypeId, Box<dyn SettableEvent>>>> =
    LazyCell::new(|| RefCell::new(HashMap::new()));

// pub fn use_state_event<M: Clone + 'static, T: Stateful + Valuable<M> + Clone + 'static>(
//     event: T,
// ) -> StateEvent<T, M>
// where
//     StateEvent<T, M>: SettableEvent,
//     <T as Stateful>::Data: DeserializeOwned + Default + Clone,
// {
//     let b = SUB;
//     let mut event_subscriptions = b.borrow_mut();
//
//     if let Some(state_event) = event_subscriptions.get(&TypeId::of::<T>()) {
//         let state_event = (*state_event)
//             .as_any()
//             .downcast_ref::<StateEvent<T, M>>()
//             .unwrap();
//
//         state_event.clone()
//     } else {
//         // let data = SignalData::new(StateInner {
//         //     inner: <T as Stateful>::EventData::default(),
//         //     on_update: None,
//         // });
//         // let state_event = StateEvent {
//         //     data: Signal {
//         //         inner: Box::into_raw(Box::new(data)),
//         //     },
//         // };
//
//         let state_event = StateEvent {
//             data: StateInner {
//                 inner: <T as Stateful>::Data::default(),
//                 on_update: None,
//                 _marker: PhantomData,
//             },
//         };
//
//         event_subscriptions.insert(TypeId::of::<T>(), Box::new(state_event.clone()));
//
//         state_event
//     }
// }

// fn test() {
//     #[derive(Clone, Copy)]
//     struct MyStateU32;
//     impl Stateful for MyStateU32 {
//         type Data = u32;
//
//         fn name() -> &'static str {
//             "myStateU32"
//         }
//     }
//     impl Valuable<IsSingleValue> for MyStateU32 {}
//
//     #[derive(Clone, Copy)]
//     struct MyStateVecU32;
//     impl Stateful for MyStateVecU32 {
//         type Data = HashMap<u32, u32>;
//
//         fn name() -> &'static str {
//             "myStateVecU32"
//         }
//     }
//
//     let _ = use_state_event(MyStateVecU32);
//     let _ = use_state_event(MyStateU32);
//
//     let b = SUB;
//     let mut event_subscriptions = b.borrow_mut();
//     for event in event_subscriptions.values_mut() {
//         event.set(json!({}));
//     }
// }
