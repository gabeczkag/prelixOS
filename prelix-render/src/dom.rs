//! Arena-based DOM: węzły w płaskim `Vec`, powiązania przez `NodeId`.

use alloc::string::String;
use alloc::vec::Vec;

pub type NodeId = usize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Element(ElementData),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementData {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub kind: NodeKind,
}

#[derive(Debug, Clone)]
pub struct Document {
    pub nodes: Vec<Node>,
}

impl Document {
    pub fn new() -> Self {
        Document { nodes: Vec::new() }
    }

    /// Korzeń dokumentu: pierwszy węzeł (tag `html`), tworzony zawsze pierwszy.
    pub fn root(&self) -> NodeId {
        0
    }

    pub fn children(&self, id: NodeId) -> &[NodeId] {
        &self.nodes[id].children
    }

    pub(crate) fn push_element(
        &mut self,
        tag: &str,
        attrs: Vec<(String, String)>,
        parent: Option<NodeId>,
    ) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node {
            parent,
            children: Vec::new(),
            kind: NodeKind::Element(ElementData {
                tag: String::from(tag),
                attrs,
            }),
        });
        if let Some(p) = parent {
            self.nodes[p].children.push(id);
        }
        id
    }

    pub(crate) fn push_text(&mut self, text: &str, parent: Option<NodeId>) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node {
            parent,
            children: Vec::new(),
            kind: NodeKind::Text(String::from(text)),
        });
        if let Some(p) = parent {
            self.nodes[p].children.push(id);
        }
        id
    }
}
