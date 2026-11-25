#![no_std]

extern crate alloc;

mod css;
mod dom;
mod html;

pub use css::*;
pub use dom::{Document, ElementData, Node, NodeId, NodeKind};
pub use html::parse_html;

mod layout;
mod style;

pub use layout::{LaidOutNode, assets_font_bytes, compute_layout};
pub use style::{Border, Style, default_style, style_tree};
