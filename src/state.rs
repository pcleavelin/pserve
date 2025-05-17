use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{any::Any, collections::HashMap, hash::Hash, marker::PhantomData};

use crate::signal::Signal;

#[cfg(not(target_arch = "wasm32"))]
use crate::server::ToClientEvent;

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
    fn as_any_mut(&mut self) -> &mut dyn Any;
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
pub struct StateEvent<T: Stateful + Clone + 'static, M: Clone + Copy>
where
    <T as Stateful>::Data: DeserializeOwned + Default + Clone,
{
    pub(crate) data: Signal<StateInner<T, M>, T::Key>,
}

#[derive(Clone, Copy)]
pub struct PartialStateEvent<T: MultipleValueUpdate + Clone + 'static>
where
    <T as Stateful>::Data: InnerCollection + DeserializeOwned + Default + Clone,
    <T::Data as InnerCollection>::Key: Serialize + DeserializeOwned,
    <T::Data as InnerCollection>::Inner: Serialize + DeserializeOwned,
{
    pub(crate) data: Signal<PartialStateInner<T>, T::Key>,
}

#[derive(Clone)]
pub struct PartialStateInner<T: MultipleValueUpdate + Clone + 'static>
where
    <T as Stateful>::Data: InnerCollection + DeserializeOwned + Default + Clone,
    <T::Data as InnerCollection>::Key: Serialize + DeserializeOwned,
    <T::Data as InnerCollection>::Inner: Serialize + DeserializeOwned,
{
    pub(crate) key: <T::Data as InnerCollection>::Key,
    pub(crate) data: <T::Data as InnerCollection>::Inner,
}

impl<T: MultipleValueUpdate + Clone> PartialStateEvent<T>
where
    <T as Stateful>::Data: InnerCollection + DeserializeOwned + Default + Clone,
    <T::Data as InnerCollection>::Key: Serialize + DeserializeOwned,
    <T::Data as InnerCollection>::Inner: Serialize + DeserializeOwned,
{
    pub fn get(&self) -> <T::Data as InnerCollection>::Inner {
        self.data.get().data
    }
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

pub trait InnerUpdate<T: Stateful, D: DeserializeOwned> {
    fn apply_update(&mut self, update: D) -> Vec<T::Key>;
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

impl<T: Stateful + Clone> InnerUpdate<T, T::Data> for StateInner<T, IsSingleValue>
where
    <T as Stateful>::Data: DeserializeOwned + Default + Clone,
{
    fn apply_update(&mut self, update: T::Data) -> Vec<T::Key> {
        self.inner = update;

        Vec::new()
    }
}

impl<T: MultipleValueUpdate + Clone> InnerUpdate<T, MultipleValueUpdateArray<T::Data>>
    for StateInner<T, IsMultipleValue>
where
    <T as Stateful>::Data: InnerCollection<Key = T::Key> + DeserializeOwned + Default + Clone,
    <T::Data as InnerCollection>::Key: Serialize + DeserializeOwned,
    <T::Data as InnerCollection>::Inner: Serialize + DeserializeOwned,
{
    fn apply_update(&mut self, update: MultipleValueUpdateArray<T::Data>) -> Vec<T::Key> {
        T::apply_update(update, &mut self.inner)
    }
}

impl<T: Stateful + Clone> SettableEvent for StateEvent<T, IsSingleValue>
where
    StateInner<T, IsSingleValue>: InnerUpdate<T, T::Data>,
    <T as Stateful>::Data: DeserializeOwned + Default + Clone,
{
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn set(&mut self, value: serde_json::Value) {
        let keys = if let Ok(value) =
            serde_json::from_value::<StatefulClientEvent<T, T::Data>>(value.clone())
        {
            if value.state_key == T::name() {
                let data = self.data.get_mut();
                data.apply_update(value.event)
            } else {
                // This wasn't the state we were looking for
                return;
            }
        } else if let Ok(value) = serde_json::from_value::<T::Data>(value) {
            let data = self.data.get_mut();
            data.apply_update(value)
        } else {
            #[cfg(target_arch = "wasm32")]
            crate::client::env::log(&format!("failed to deserialize single value update"));
            return;
        };

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
    StateInner<T, IsMultipleValue>: InnerUpdate<T, MultipleValueUpdateArray<T::Data>>,
    <T as Stateful>::Data: InnerCollection + DeserializeOwned + Default + Clone,
    <T::Data as InnerCollection>::Key: Serialize + DeserializeOwned,
    <T::Data as InnerCollection>::Inner: Serialize + DeserializeOwned,
{
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn set(&mut self, value: serde_json::Value) {
        let keys = if let Ok(value) = serde_json::from_value::<
            StatefulClientEvent<T, MultipleValueUpdateArray<T::Data>>,
        >(value.clone())
        {
            if value.state_key == T::name() {
                let data = self.data.get_mut();
                data.apply_update(value.event)
            } else {
                // This wasn't the state we were looking for
                return;
            }
        } else if let Ok(value) = serde_json::from_value::<MultipleValueUpdateArray<T::Data>>(value)
        {
            let data = self.data.get_mut();
            data.apply_update(value)
        } else {
            #[cfg(target_arch = "wasm32")]
            crate::client::env::log(&format!("failed to deserialize multiple value update"));
            return;
        };

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

impl<T: MultipleValueUpdate + Clone> SettableEvent for PartialStateEvent<T>
where
    <T as Stateful>::Data: InnerCollection + DeserializeOwned + Default + Clone,
    <T::Data as InnerCollection>::Key: Serialize + DeserializeOwned + PartialEq<T::Key> + PartialEq,
    <T::Data as InnerCollection>::Inner: Serialize + DeserializeOwned,
    T::Key: PartialEq<<T::Data as InnerCollection>::Key>,
{
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn set(&mut self, value: serde_json::Value) {
        let our_key = if let Ok(value) = serde_json::from_value::<
            StatefulClientEvent<T, MultipleValueUpdateArray<T::Data>>,
        >(value.clone())
        {
            if value.state_key == T::name() {
                let data = self.data.get_mut();
                if let Some(value) = value
                    .event
                    .iter()
                    .find(|(event_key, _)| *event_key == data.key)
                    .map(|(_, event_value)| event_value)
                {
                    data.data = value.clone();
                    data.key.clone()
                } else {
                    return;
                }
            } else {
                // This wasn't the state we were looking for
                return;
            }
        } else {
            #[cfg(target_arch = "wasm32")]
            crate::client::env::log(&format!("failed to deserialize multiple value update"));
            return;
        };

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
                        .filter(|(key, _)| our_key == **key)
                    {
                        to_re_render.insert(*dom_id);
                    }
                }
            }
        }
    }
}
