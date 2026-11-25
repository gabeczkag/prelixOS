//! Computed style per węzeł DOM: dopasowanie reguł CSS + dziedziczenie.

use alloc::vec::Vec;

use crate::css::{Color, Declaration, FlexDirection, Length, Sides, Selector, Stylesheet};
use crate::dom::{Document, NodeId, NodeKind};

#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub background_color: Color,
    pub color: Color,
    pub font_size: f32,
    pub display_flex: bool,
    pub flex_direction: FlexDirection,
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub padding: Sides<Length>,
    pub margin: Sides<Length>,
    pub border: Option<Border>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Border {
    pub width: f32,
    pub color: Color,
}

pub fn default_style() -> Style {
    let zero = Length::Px(0.0);
    Style {
        background_color: Color { r: 0, g: 0, b: 0, a: 0 },
        color: Color { r: 0, g: 0, b: 0, a: 255 },
        font_size: 16.0,
        display_flex: false,
        flex_direction: FlexDirection::Row,
        width: None,
        height: None,
        padding: Sides { left: zero, top: zero, right: zero, bottom: zero },
        margin: Sides { left: zero, top: zero, right: zero, bottom: zero },
        border: None,
    }
}

pub fn style_tree(doc: &Document, sheet: &Stylesheet, root: NodeId) -> Vec<Style> {
    let mut styles: Vec<Style> = doc.nodes.iter().map(|_| default_style()).collect();
    if root >= doc.nodes.len() {
        return styles;
    }

    // Jawny stos zamiast rekursji: brak ryzyka przepełnienia stosu na
    // głęboko zagnieżdżonym drzewie. Pre-order; rodzic jest liczony przed
    // dziećmi, więc dziecko czyta skądś style[parent].
    let mut stack: Vec<NodeId> = Vec::new();
    stack.push(root);
    while let Some(id) = stack.pop() {
        let node = &doc.nodes[id];
        let parent = node.parent.filter(|p| *p < id && *p < doc.nodes.len());

        // Dziedziczą wyłącznie `color` i `font_size`.
        let mut style = match parent.map(|p| &styles[p]) {
            Some(p) => {
                let mut s = default_style();
                s.color = p.color;
                s.font_size = p.font_size;
                s
            }
            None => default_style(),
        };

        if let NodeKind::Element(e) = &node.kind {
            for rule in &sheet.rules {
                let matches = match &rule.selector {
                    Selector::Universal => true,
                    Selector::Tag(tag) => e.tag.eq_ignore_ascii_case(tag),
                };
                if !matches {
                    continue;
                }
                apply(&mut style, &rule.declarations);
            }
        }

        styles[id] = style;

        for &child in &node.children {
            stack.push(child);
        }
    }
    styles
}

fn apply(style: &mut Style, declarations: &[Declaration]) {
    for decl in declarations {
        match *decl {
            Declaration::BackgroundColor(c) => style.background_color = c,
            Declaration::Color(c) => style.color = c,
            Declaration::FontSize(size) => style.font_size = size,
            Declaration::Width(l) => style.width = Some(l),
            Declaration::Height(l) => style.height = Some(l),
            Declaration::Padding(s) => style.padding = s,
            Declaration::Margin(s) => style.margin = s,
            Declaration::Border { width, color } => style.border = Some(Border { width, color }),
            Declaration::DisplayFlex => style.display_flex = true,
            Declaration::FlexDirection(dir) => style.flex_direction = dir,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::parse_stylesheet;
    use crate::dom::NodeKind;
    use crate::html::parse_html;

    fn child_of(doc: &Document, id: NodeId, i: usize) -> NodeId {
        doc.children(id)[i]
    }

    #[test]
    fn applies_matching_tag_rules() {
        let doc = parse_html("<div></div><span></span>");
        let sheet = parse_stylesheet("div { background-color: red; }");
        let styles = style_tree(&doc, &sheet, doc.root());
        let div = child_of(&doc, doc.root(), 0);
        let span = child_of(&doc, doc.root(), 1);
        assert_eq!(
            styles[div].background_color,
            Color { r: 255, g: 0, b: 0, a: 255 }
        );
        assert_eq!(styles[span], default_style());
    }

    #[test]
    fn universal_rule_hits_everything() {
        let doc = parse_html("<div></div><span></span>");
        let sheet = parse_stylesheet("* { color: blue; }");
        let styles = style_tree(&doc, &sheet, doc.root());
        let div = child_of(&doc, doc.root(), 0);
        let span = child_of(&doc, doc.root(), 1);
        assert_eq!(styles[div].color, Color { r: 0, g: 0, b: 255, a: 255 });
        assert_eq!(styles[span].color, Color { r: 0, g: 0, b: 255, a: 255 });
    }

    #[test]
    fn color_and_font_size_inherit() {
        let doc = parse_html("<div>text</div>");
        let sheet = parse_stylesheet("div { color: red; font-size: 32px; }");
        let styles = style_tree(&doc, &sheet, doc.root());
        let div = child_of(&doc, doc.root(), 0);
        let text = child_of(&doc, div, 0);
        assert!(matches!(
            &doc.nodes[text].kind,
            NodeKind::Text(_)
        ));
        assert_eq!(styles[text].color, Color { r: 255, g: 0, b: 0, a: 255 });
        assert_eq!(styles[text].font_size, 32.0);
    }

    #[test]
    fn later_rule_wins() {
        let doc = parse_html("<div></div>");
        let sheet = parse_stylesheet("div { color: red; } div { color: blue; }");
        let styles = style_tree(&doc, &sheet, doc.root());
        let div = child_of(&doc, doc.root(), 0);
        assert_eq!(styles[div].color, Color { r: 0, g: 0, b: 255, a: 255 });
    }
}
