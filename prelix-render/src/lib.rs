#![no_std]

extern crate alloc;

mod css;
mod dom;
mod html;

pub use css::*;
pub use dom::{Document, ElementData, Node, NodeId, NodeKind};
pub use html::parse_html;

mod style;

pub use style::{Border, Style, default_style, style_tree};
