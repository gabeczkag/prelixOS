#![no_std]

extern crate alloc;

mod dom;
mod html;

pub use dom::{Document, ElementData, Node, NodeId, NodeKind};
pub use html::parse_html;
