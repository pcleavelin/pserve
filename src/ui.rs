pub struct State {
    pub(crate) elements: Tree<256, Element>,
}

pub struct Tree<const N: usize, T> {
    pub(crate) items: Box<[TreeItem<T>; N]>,
    pub(crate) curr_parent: Option<usize>,
    pub(crate) len: usize,
}

#[derive(Clone, Debug)]
pub struct TreeItem<T> {
    pub(crate) first: Option<usize>,
    pub(crate) last: Option<usize>,
    pub(crate) next: Option<usize>,
    pub(crate) prev: Option<usize>,
    pub(crate) parent: Option<usize>,

    pub(crate) data: T,
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

#[derive(Default, Clone, Debug)]
pub struct Element {
    pub(crate) kind: ElementKind,
    pub(crate) layout: Layout,
}

impl Element {
    pub fn new(kind: ElementKind, layout: Layout) -> Self {
        Self { kind, layout }
    }
}

#[derive(Default, Clone, Debug)]
pub enum ElementKind {
    #[default]
    Container,
    Text(String),
    Image(u32),
    // TODO:
    // Custom
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
        [self[0] - self[0], self[1] - self[1]]
    }
}

#[derive(Default, Clone, Debug)]
pub struct Layout {
    pub(crate) dir: Direction,

    pub(crate) pos: [i32; 2],
    pub(crate) size: [Size; 2],
}

#[derive(Default, Clone, Copy, Debug)]
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

#[derive(Default, Clone, Copy, Debug)]
pub struct Size {
    kind: SizeKind,
    pub(crate) value: i32,
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
    layout: Layout,
    // TODO: clicked, hovered, etc.
}

impl State {
    pub fn new() -> Self {
        Self {
            elements: Tree::new(),
        }
    }

    pub fn reset(&mut self) {
        self.elements.clear();
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
                                prev.data.layout.pos.x() + prev.data.layout.size.x().value;
                            *e.data.layout.pos.y_mut() = parent.data.layout.pos.y();
                        }
                        Direction::TopToBottom => {
                            *e.data.layout.pos.x_mut() = parent.data.layout.pos.x();
                            *e.data.layout.pos.y_mut() =
                                prev.data.layout.pos.y() + prev.data.layout.size.y().value;
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
            // dumb way of not doing a memory allocation here
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

    pub fn open_element(&mut self, kind: ElementKind, layout: Layout) {
        self.elements.push(Element::new(kind, layout));
    }

    pub fn close_element(&mut self) -> Interaction {
        // TODO: change this to a proper impl on `Tree`
        let mut e = self.elements.curr_parent();
        e.layout.size.set_zero();

        {
            let size_x = e.layout.size.x_mut();
            match size_x.kind {
                SizeKind::Fit => {
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
                                            size_x.value += child.data.layout.size.x().value
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
                                            size_y.value += child.data.layout.size.y().value
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
            ..Default::default()
        };

        self.elements.update_parent(e);
        self.elements.step_up();

        interaction
    }
}
