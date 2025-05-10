use std::{
    any::{Any, TypeId},
    cell::{LazyCell, RefCell},
    collections::HashMap,
    hash::Hash,
    marker::PhantomData,
};

use serde::de::DeserializeOwned;
use serde_json::json;

use crate::signal::Signal;

pub trait Stateful {
    type Data: DeserializeOwned + Default + Clone;

    fn name() -> &'static str;
}

pub trait SettableEvent {
    fn as_any(&self) -> &dyn Any;
    fn set(&mut self, value: serde_json::Value);
}

pub trait InnerCollection {
    type Key: DeserializeOwned + Default + Clone;
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
        self[key as usize] = value;
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
    pub(crate) data: Signal<StateInner<T, M>>,
}

type MultipleValueUpdateArray<T> =
    Vec<(<T as InnerCollection>::Key, <T as InnerCollection>::Inner)>;
pub trait MultipleValueUpdate
where
    Self: Stateful,
    Self::Data: InnerCollection,
{
    fn apply_update(update: MultipleValueUpdateArray<Self::Data>, data: &mut Self::Data);
}

impl<T> MultipleValueUpdate for T
where
    T: Stateful,
    T::Data: InnerCollection,
{
    fn apply_update(update: MultipleValueUpdateArray<Self::Data>, data: &mut Self::Data) {
        for (key, value) in update {
            data.set_at(key, value);
        }
    }
}

#[derive(Clone, Copy)]
pub struct IsSingleValue;
#[derive(Clone, Copy)]
pub struct IsMultipleValue;

pub trait InnerUpdate {
    fn apply_update(&mut self, update: serde_json::Value);
}
pub trait Valuable<T> {}

impl<T> Valuable<IsMultipleValue> for T
where
    T: Stateful + MultipleValueUpdate,
    T::Data: InnerCollection,
{
}

#[derive(Clone, Copy)]
pub struct StateInner<T: Stateful + Clone + 'static, M> {
    pub(crate) inner: T::Data,
    pub(crate) on_update: Option<fn(&T::Data)>,
    pub(crate) _marker: PhantomData<M>,
}

impl<T: Stateful + Clone> InnerUpdate for StateInner<T, IsSingleValue>
where
    <T as Stateful>::Data: DeserializeOwned + Default + Clone,
{
    fn apply_update(&mut self, update: serde_json::Value) {
        #[cfg(target_arch = "wasm32")]
        crate::client::env::log(&format!("setting single value: {update:?}"));

        if let Ok(value) = serde_json::from_value::<<T as Stateful>::Data>(update) {
            #[cfg(target_arch = "wasm32")]
            crate::client::env::log("successfully deserialized single value update");

            self.inner = value;
        } else {
            #[cfg(target_arch = "wasm32")]
            crate::client::env::log("couldn't deserialize single value update");
        }
    }
}

impl<T: MultipleValueUpdate + Clone> InnerUpdate for StateInner<T, IsMultipleValue>
where
    <T as Stateful>::Data: InnerCollection + DeserializeOwned + Default + Clone,
{
    fn apply_update(&mut self, update: serde_json::Value) {
        #[cfg(target_arch = "wasm32")]
        crate::client::env::log(&format!("setting multiple values: {update:?}"));

        if let Ok(value) = serde_json::from_value::<MultipleValueUpdateArray<T::Data>>(update) {
            #[cfg(target_arch = "wasm32")]
            crate::client::env::log("successfully deserialized multiple value update");

            T::apply_update(value, &mut self.inner);
        } else {
            #[cfg(target_arch = "wasm32")]
            crate::client::env::log("couldn't deserialize multiple value update");
        }
    }
}

impl<T: Stateful + Clone> SettableEvent for StateEvent<T, IsSingleValue>
where
    StateInner<T, IsSingleValue>: InnerUpdate,
    <T as Stateful>::Data: DeserializeOwned + Default + Clone,
{
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn set(&mut self, value: serde_json::Value) {
        let data = self.data.get_mut();
        data.apply_update(value);
    }
}

impl<T: Stateful + MultipleValueUpdate + Clone> SettableEvent for StateEvent<T, IsMultipleValue>
where
    StateInner<T, IsMultipleValue>: InnerUpdate,
    <T as Stateful>::Data: InnerCollection + DeserializeOwned + Default + Clone,
{
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn set(&mut self, value: serde_json::Value) {
        let data = self.data.get_mut();
        data.apply_update(value);
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
