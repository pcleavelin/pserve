use std::collections::HashMap;

#[derive(Default)]
pub struct DomNodeBuilder {
    children: Vec<DomNodeUnbuilt>,
}

pub struct DomNodeUnbuilt {
    pub id: u32,
    pub tag: &'static str,
    pub attributes: Vec<(String, String)>,
    pub body: Option<DomNodeUnbuiltBody>,
    pub on_input: Option<Box<Box<dyn Fn(&str)>>>,
    pub on_click: Option<Box<Box<dyn Fn(&str)>>>,
}

pub struct DomNodeBuilt {
    pub id: u32,
    pub body: DomNodeBuiltBody,
}

pub enum DomNodeBuiltBody {
    Text(String),
    Nodes(Vec<u32>),
}

pub enum DomNodeUnbuiltBody {
    Text(String),
    Constructor(Box<Box<dyn Fn() -> DomNodeBuilder>>),
}

impl DomNodeBuilder {
    pub fn push(mut self, tag: &'static str, body: impl Fn() -> DomNodeBuilder + 'static) -> Self {
        self.children.push(DomNodeUnbuilt {
            #[cfg(target_arch = "wasm32")]
            id: crate::client::next_dom_id(),
            #[cfg(not(target_arch = "wasm32"))]
            id: 0,
            tag,
            attributes: Vec::new(),
            body: Some(DomNodeUnbuiltBody::Constructor(Box::new(Box::new(body)))),
            on_input: None,
            on_click: None,
        });
        self
    }

    pub fn on_click(mut self, f: impl Fn(&str) + 'static) -> Self {
        if let Some(last) = self.children.last_mut() {
            last.on_click = Some(Box::new(Box::new(f)));
        }
        self
    }

    pub fn on_input(mut self, f: impl Fn(&str) + 'static) -> Self {
        if let Some(last) = self.children.last_mut() {
            last.on_input = Some(Box::new(Box::new(f)));
        }
        self
    }

    pub fn attr(mut self, key: impl ToString, value: impl ToString) -> Self {
        if let Some(last) = self.children.last_mut() {
            last.attributes.push((key.to_string(), value.to_string()));
        }
        self
    }

    pub fn build(
        self,
        unbuilt_nodes: &mut HashMap<u32, DomNodeUnbuilt>,
        built_nodes: &mut HashMap<u32, DomNodeBuilt>,
        run_children: bool,
    ) -> Vec<u32> {
        let mut built = Vec::new();

        for child in self.children {
            let child_id = child.id;

            if let Some(body) = &child.body {
                match body {
                    DomNodeUnbuiltBody::Text(text) => {
                        built_nodes.insert(
                            child.id,
                            DomNodeBuilt {
                                id: child.id,
                                body: DomNodeBuiltBody::Text(text.clone()),
                            },
                        );
                        unbuilt_nodes.insert(child.id, child);
                    }
                    DomNodeUnbuiltBody::Constructor(ctor) if run_children => {
                        #[cfg(target_arch = "wasm32")]
                        crate::client::set_current_dom_id(child.id);

                        let builder = ctor();
                        let child_body = builder.build(unbuilt_nodes, built_nodes, run_children);

                        #[cfg(target_arch = "wasm32")]
                        crate::client::set_current_dom_id(0);

                        built_nodes.insert(
                            child.id,
                            DomNodeBuilt {
                                id: child.id,
                                body: DomNodeBuiltBody::Nodes(child_body),
                            },
                        );
                        unbuilt_nodes.insert(child.id, child);
                    }
                    DomNodeUnbuiltBody::Constructor(_) => {}
                }
            }

            built.push(child_id);
        }

        built
    }
}

impl<T: AsRef<str>> From<T> for DomNodeBuilder {
    fn from(value: T) -> Self {
        Self {
            children: vec![DomNodeUnbuilt {
                #[cfg(target_arch = "wasm32")]
                id: crate::client::next_dom_id(),
                #[cfg(not(target_arch = "wasm32"))]
                id: 0,
                tag: "",
                attributes: Vec::new(),
                body: Some(DomNodeUnbuiltBody::Text(value.as_ref().to_string())),
                on_input: None,
                on_click: None,
            }],
        }
    }
}
