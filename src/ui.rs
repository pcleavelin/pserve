use std::{any::TypeId, ops::{Deref, Not}};

pub struct State {
    pub elements: Tree<256, Element>,
    user_data_arena: GenericArena,

    pub last_frame_elements: Tree<256, Element>,
    last_frame_user_data_arena: GenericArena,

    pub input: [Input; 256],
    pub last_input: [Input; 256],
}

#[derive(Default, Clone)]
pub struct Input {
    pub mouse_down: bool,
    pub inputted_text: String,
}

#[derive(Clone)]
pub struct Tree<const N: usize, T> {
    items: Box<[TreeItem<T>; N]>,
    pub(crate) curr_parent: Option<usize>,
    pub len: usize,
}

#[derive(Clone, Debug)]
pub struct TreeItem<T> {
    pub(crate) first: Option<usize>,
    pub(crate) last: Option<usize>,
    pub(crate) next: Option<usize>,
    pub(crate) prev: Option<usize>,
    pub(crate) parent: Option<usize>,

    pub data: T,
}

impl<T> Deref for TreeItem<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

#[repr(C, packed)]
pub(crate) struct UserDataItem<T> {
    ty: TypeId,
    data: T,
}

pub trait Vector2Like<T> {
    fn x(&self) -> T;
    fn y(&self) -> T;

    fn x_mut(&mut self) -> &mut T;
    fn y_mut(&mut self) -> &mut T;

    fn set_zero(&mut self);

    fn sub(&self, other: &Self) -> Self;
}

impl<T: Default + Copy> Vector2Like<T> for [T; 2]
where
    T: std::ops::Sub<Output = T>,
{
    fn x(&self) -> T {
        self[0]
    }

    fn y(&self) -> T {
        self[1]
    }

    fn x_mut(&mut self) -> &mut T {
        &mut self[0]
    }

    fn y_mut(&mut self) -> &mut T {
        &mut self[1]
    }

    fn set_zero(&mut self) {
        *self = [T::default(); 2]
    }

    fn sub(&self, other: &Self) -> Self {
        [self[0] - other[0], self[1] - other[1]]
    }
}

impl<T> TreeItem<T> {
    fn new(data: T) -> Self {
        Self {
            first: None,
            last: None,
            next: None,
            prev: None,
            parent: None,
            data,
        }
    }
}

impl<const N: usize, T: Default + Clone + std::fmt::Debug> Tree<N, T> {
    pub fn new() -> Self {
        Self {
            items: vec![TreeItem::new(T::default()); N].try_into().unwrap(),
            curr_parent: None,
            len: 0,
        }
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        if index < self.len {
            Some(&self.items[index].data)
        } else {
            None
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn clear(&mut self) {
        self.curr_parent = None;
        self.len = 0;
    }

    pub fn curr_parent(&mut self) -> T {
        if let Some(parent) = self.curr_parent {
            self.items[parent].data.clone()
        } else {
            panic!("attempted to access empty tree");
        }
    }

    pub fn update_parent(&mut self, data: T) {
        self.items[self.curr_parent.expect("tree to not be empty")].data = data;
    }

    fn update_item(&mut self, index: usize, data: T) {
        self.items[index].data = data;
    }

    pub fn push(&mut self, data: T) -> usize {
        let mut new_item = TreeItem::new(data);

        if let Some(parent) = self.curr_parent {
            new_item.parent = Some(parent);

            if let Some(last) = self.items[parent].last {
                new_item.prev = Some(last);

                self.items[last].next = Some(self.len);
            }

            self.items[parent].last = Some(self.len);

            if self.items[parent].first.is_none() {
                self.items[parent].first = Some(self.len);
            }
        }

        self.items[self.len] = new_item;
        self.curr_parent = Some(self.len);
        self.len += 1;

        self.len - 1
    }

    pub fn step_up(&mut self) {
        if let Some(parent) = self.curr_parent {
            self.curr_parent = self.items[parent].parent;
        } else {
            panic!("tried stepping up when no parent exists");
        }
    }
}

pub trait GenericArenaItem {
    fn cloned_bytes(&self) -> Vec<u8>;
}

pub struct GenericArena {
    types: Vec<(TypeId, usize, *const ())>,
    arena: Vec<u8>,
}

impl GenericArena {
    fn new() -> Self {
        Self {
            types: Vec::new(),
            arena: Vec::new(),
        }
    }

    fn push<T: GenericArenaItem + 'static>(&mut self, user_data: T) -> Option<usize> {
        if TypeId::of::<T>() == TypeId::of::<()>() {
            return None;
        }

        // trick the compiler into giving us a pointer to T's vtable for GenericArenaItem
        let vtable = {
            let fat: &dyn GenericArenaItem = &user_data;
            let fat_bytes: [usize; 2] = unsafe { std::mem::transmute(fat) };

            fat_bytes
        }[1] as *const ();

        let bytes = unsafe {
            std::slice::from_raw_parts(
                &user_data as *const T as *const u8,
                std::mem::size_of::<T>(),
            )
        };

        // we have "moved" user_data into the arena, but the compiler doesn't know that so we need
        // to tell it not to run `drop()` on it
        std::mem::forget(user_data);

        let byte_index = self.arena.len();
        let index = self.types.len();

        self.arena.extend_from_slice(bytes);
        self.types.push((TypeId::of::<T>(), byte_index, vtable));

        Some(index)
    }

    pub fn get<T: Clone + 'static>(&self, index: usize) -> Option<&T> {
        let (ty, byte_index, _) = self.types[index];
        if ty != TypeId::of::<T>() {
            return None;
        }

        let slice = self.arena[byte_index..].as_ptr();
        let item: *const T = unsafe { std::mem::transmute(slice) };

        if ty == TypeId::of::<T>() {
            unsafe {
                return Some(&*item);
            }
        }

        None
    }

    pub fn clear(&mut self) {
        for index in 0..self.types.len() {
            let item = self.get_dyn(index);

            // run `drop()` on the item
            let _ = Box::from(item);
        }

        self.types.clear();
        self.arena.clear();
    }

    fn push_raw(&mut self, ty: TypeId, bytes: Vec<u8>, vtable: *const ()) {
        let byte_index = self.arena.len();

        self.arena.extend(bytes);
        self.types.push((ty, byte_index, vtable));
    }

    fn get_dyn(&self, index: usize) -> &dyn GenericArenaItem {
        let (_, byte_index, vtable) = self.types[index];

        let bytes = self.arena[byte_index..].as_ptr();
        let fat_bytes: [usize; 2] = [bytes as usize, vtable as usize];
        let fat_ptr: &dyn GenericArenaItem = unsafe { std::mem::transmute(fat_bytes) };

        fat_ptr
    }

    fn get_cloned_bytes(&self, index: usize) -> Vec<u8> {
        let fat_ptr = self.get_dyn(index);

        fat_ptr.cloned_bytes()
    }
}

impl Drop for GenericArena {
    fn drop(&mut self) {
        self.clear();
    }
}

impl Clone for GenericArena {
    fn clone(&self) -> Self {
        let mut new_arena = Self::new();

        for (index, (ty, _, vtable)) in self.types.iter().enumerate() {
            let cloned_bytes = self.get_cloned_bytes(index);

            new_arena.push_raw(*ty, cloned_bytes, *vtable);
        }

        new_arena
    }
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct Element {
    pub(crate) kind: ElementKind,
    pub(crate) layout: Layout,
    pub user_data: Option<usize>,
}

impl Element {
    pub fn new<T: GenericArenaItem + std::fmt::Debug + 'static>(
        state: &mut State,
        kind: ElementKind,
        layout: Layout,
        user_data: T,
    ) -> Self {
        let user_data_index = state.user_data_arena.push(user_data);

        Self {
            kind,
            layout,
            user_data: user_data_index,
        }
    }
}

#[derive(Default, Clone, Debug, PartialEq)]
pub enum ElementKind {
    #[default]
    Container,
    Text(String),
    Image(u32),
    // TODO:
    // Custom
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct Layout {
    pub(crate) dir: Direction,

    pub(crate) pos: [i32; 2],
    pub(crate) size: [Size; 2],
}

impl Layout {
    pub fn size(size: [Size; 2]) -> Self {
        Self {
            size,
            ..Default::default()
        }
    }
}

#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    #[default]
    LeftToRight,
    TopToBottom,
}

impl From<Direction> for Layout {
    fn from(value: Direction) -> Self {
        Self {
            dir: value,
            pos: [0; 2],
            size: [Size::default(); 2],
        }
    }
}

#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct Size {
    kind: SizeKind,
    pub(crate) value: i32,
}

impl Size {
    pub fn exact(value: i32) -> Self {
        Self {
            kind: SizeKind::Exact,
            value,
        }
    }

    pub fn grow() -> Self {
        Self {
            kind: SizeKind::Grow,
            value: 0,
        }
    }

    pub fn fit() -> Self {
        Self {
            kind: SizeKind::Fit,
            value: 0,
        }
    }
}

impl std::ops::Sub for Size {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        if self.kind != rhs.kind {
            panic!(
                "std::ops::Sub can only be applied on Size if both kinds are of the same variant"
            )
        }

        Self {
            kind: self.kind,
            value: self.value - rhs.value,
        }
    }
}

#[derive(Default, Clone, Copy, PartialEq, Debug)]
pub enum SizeKind {
    #[default]
    Fit,
    Grow,
    Exact,
}

#[derive(Default)]
pub struct Interaction {
    pub layout: Layout,
    pub clicked: bool,
    pub inputted_text: String,

    // TODO: hovered, etc.
}

#[derive(Debug, Clone)]
pub enum HtmlElementType {
    Button,
    TextBox,
    Link(String),
}

impl<T: Clone> GenericArenaItem for T {
    fn cloned_bytes(&self) -> Vec<u8>
    where
        Self: Sized,
    {
        let item = self.clone();

        let mut bytes = Vec::new();
        bytes.extend_from_slice(unsafe {
            std::slice::from_raw_parts(&item as *const _ as *const u8, std::mem::size_of::<Self>())
        });
        std::mem::forget(item);

        bytes
    }
}

impl State {
    pub fn new() -> Self {
        Self {
            elements: Tree::new(),
            user_data_arena: GenericArena::new(),

            last_frame_elements: Tree::new(),
            last_frame_user_data_arena: GenericArena::new(),

            input: core::array::from_fn(|_| Input::default()),
            last_input: core::array::from_fn(|_| Input::default()),
        }
    }

    pub fn next_frame(&mut self) {
        self.last_frame_elements = self.elements.clone();
        self.last_frame_user_data_arena = self.user_data_arena.clone();
        self.last_input = self.input.clone();

        self.elements.clear();
        self.user_data_arena.clear();
    }

    pub fn get_user_data<T: GenericArenaItem + std::fmt::Debug + Clone + 'static>(
        &self,
        index: usize,
    ) -> Option<&T> {
        self.user_data_arena.get(index)
    }

    pub fn compute_layout(&mut self) {
        self.grow_children(0);

        for i in 0..self.elements.len {
            let mut e = self.elements.items[i].clone();

            if let Some(parent_index) = e.parent {
                let parent = &self.elements.items[parent_index];

                if let Some(prev_index) = e.prev {
                    let prev = &self.elements.items[prev_index];

                    match parent.data.layout.dir {
                        Direction::LeftToRight => {
                            *e.data.layout.pos.x_mut() =
                                // TODO: change the `8` to a `padding` value
                                prev.data.layout.pos.x() + prev.data.layout.size.x().value + 8;
                            *e.data.layout.pos.y_mut() = parent.data.layout.pos.y();
                        }
                        Direction::TopToBottom => {
                            *e.data.layout.pos.x_mut() = parent.data.layout.pos.x();
                            *e.data.layout.pos.y_mut() =
                                // TODO: change the `8` to a `padding` value
                                prev.data.layout.pos.y() + prev.data.layout.size.y().value + 8;
                        }
                    }
                } else {
                    match parent.data.layout.dir {
                        Direction::LeftToRight => {
                            // TODO: padding in the x direction
                            e.data.layout.pos = parent.data.layout.pos;
                        }
                        Direction::TopToBottom => {
                            // TODO: padding in the y direction
                            e.data.layout.pos = parent.data.layout.pos;
                        }
                    }
                }
            }

            self.elements.update_item(i, e.data);
        }
    }

    fn grow_children(&mut self, index: usize) {
        let e = self.elements.items[index].clone();

        let mut children_size = [0i32; 2];
        let mut num_growing = [0i32; 2];

        // TODO: do a proper iterator here
        let mut child_index = self.elements.items[index].first;
        loop {
            if let Some(index) = child_index {
                let child = &self.elements.items[index];
                child_index = child.next;

                if let SizeKind::Grow = child.data.layout.size.x().kind {
                    *num_growing.x_mut() += 1;
                }
                if let SizeKind::Grow = child.data.layout.size.y().kind {
                    *num_growing.y_mut() += 1;
                }

                match e.data.layout.dir {
                    Direction::LeftToRight => {
                        *children_size.x_mut() += child.data.layout.size.x().value
                    }
                    Direction::TopToBottom => {
                        *children_size.y_mut() += child.data.layout.size.y().value
                    }
                }
            } else {
                break;
            }
        }

        if num_growing.x() > 0 || num_growing.y() > 0 {
            let remaining_size = [
                e.data.layout.size.x().value - children_size.x(),
                e.data.layout.size.y().value - children_size.y(),
            ];

            let to_grow = [
                if num_growing.x() < 1 {
                    0
                } else {
                    remaining_size.x() / num_growing.x()
                },
                if num_growing.y() < 1 {
                    0
                } else {
                    remaining_size.y() / num_growing.y()
                },
            ];

            // TODO: do a proper iterator here
            let mut child_index = self.elements.items[index].first;
            loop {
                if let Some(index) = child_index {
                    let mut child = self.elements.items[index].clone();
                    child_index = child.next;

                    match e.data.layout.dir {
                        Direction::LeftToRight => {
                            if let SizeKind::Grow = child.data.layout.size.x().kind {
                                child.data.layout.size.x_mut().value = to_grow.x();
                            }
                            if let SizeKind::Grow = child.data.layout.size.y().kind {
                                child.data.layout.size.y_mut().value = remaining_size.y();
                            }
                        }
                        Direction::TopToBottom => {
                            if let SizeKind::Grow = child.data.layout.size.x().kind {
                                child.data.layout.size.x_mut().value = remaining_size.x();
                            }
                            if let SizeKind::Grow = child.data.layout.size.y().kind {
                                child.data.layout.size.y_mut().value = to_grow.y();
                            }
                        }
                    }

                    let growing = matches!(child.data.layout.size.x().kind, SizeKind::Grow)
                        || matches!(child.data.layout.size.y().kind, SizeKind::Grow);

                    self.elements.update_item(index, child.data);

                    if growing {
                        self.grow_children(index);
                    }
                } else {
                    break;
                }
            }
        }
    }

    fn is_clicked(&self, index: usize) -> bool {
        self.last_input[index].mouse_down && self.input[index].mouse_down.not()
    }

    pub fn open_element<T: GenericArenaItem + std::fmt::Debug + 'static>(
        &mut self,
        kind: ElementKind,
        layout: Layout,
        user_data: T,
    ) {
        let e = Element::new(self, kind, layout, user_data);
        self.elements.push(e);
    }

    pub fn close_element(&mut self) -> Interaction {
        let mut e = self.elements.curr_parent();

        {
            let size_x = e.layout.size.x_mut();
            match size_x.kind {
                SizeKind::Fit => {
                    size_x.value = 0;

                    match &e.kind {
                        ElementKind::Container => {
                            // TODO: turn this into an ergonomic iterator
                            let mut child_index =
                                self.elements.items[self.elements.curr_parent.unwrap()].first;
                            loop {
                                if let Some(index) = child_index {
                                    let child = &self.elements.items[index];
                                    child_index = child.next;

                                    match e.layout.dir {
                                        Direction::LeftToRight => {
                                            size_x.value += child.data.layout.size.x().value + 8
                                        }
                                        Direction::TopToBottom => {
                                            size_x.value =
                                                size_x.value.max(child.data.layout.size.x().value)
                                        }
                                    }
                                } else {
                                    break;
                                }
                            }
                        }
                        ElementKind::Text(t) => {
                            // FIXME: change this to use proper font size
                            size_x.value = (t.len() as i32) * 9;
                        }
                        ElementKind::Image(_) => todo!("images not supported yet"),
                    }
                }
                SizeKind::Grow => { /* Done in a different pass */ }
                SizeKind::Exact => { /* Value is already set */ }
            }
        }

        {
            let size_y = e.layout.size.y_mut();

            match size_y.kind {
                SizeKind::Fit => {
                    size_y.value = 0;

                    match &e.kind {
                        ElementKind::Container => {
                            // TODO: turn this into an ergonomic iterator
                            let mut child_index =
                                self.elements.items[self.elements.curr_parent.unwrap()].first;
                            loop {
                                if let Some(index) = child_index {
                                    let child = &self.elements.items[index];
                                    child_index = child.next;

                                    match e.layout.dir {
                                        Direction::LeftToRight => {
                                            size_y.value =
                                                size_y.value.max(child.data.layout.size.y().value)
                                        }
                                        Direction::TopToBottom => {
                                            size_y.value += child.data.layout.size.y().value + 8
                                        }
                                    }
                                } else {
                                    break;
                                }
                            }
                        }
                        ElementKind::Text(_) => {
                            // FIXME: change this to use proper font size
                            size_y.value = 16;
                        }
                        ElementKind::Image(_) => todo!("images not supported yet"),
                    }
                }
                SizeKind::Grow => { /* Done in a different pass */ }
                SizeKind::Exact => { /* Value is already set */ }
            }
        }

        let interaction = Interaction {
            layout: e.layout.clone(),
            clicked: self.is_clicked(self.elements.curr_parent.unwrap()),
            inputted_text: self.input[self.elements.curr_parent.unwrap()].inputted_text.clone(),
        };

        self.elements.update_parent(e);
        self.elements.step_up();

        interaction
    }
}

pub mod html {
    use super::*;

    pub trait HtmlExt {
        fn label(&mut self, text: impl ToString);
        fn button(&mut self, text: impl ToString) -> Interaction;
    }

    impl HtmlExt for State {
        fn label(&mut self, text: impl ToString) {
            self.open_element(ElementKind::Text(text.to_string()), Layout::default(), ());
            self.close_element();
        }

        fn button(&mut self, text: impl ToString) -> Interaction {
            self.open_element(ElementKind::Text(text.to_string()), Layout::default(), HtmlElementType::Button);
            self.close_element()
        }
    }
}
