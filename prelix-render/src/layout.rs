//! Layout: DOM + style tree → taffy flexbox → pozycje i rozmiary w pre-orderze.

use alloc::vec::Vec;

use taffy::style_helpers::TaffyAuto;
use taffy::{AvailableSpace, Display, LayoutInput, LayoutOutput, LengthPercentage, LengthPercentageAuto, NodeId as TaffyId, Rect as TaffyRect, Size as TaffySize, Style as TaffyStyle, TaffyTree};

use crate::css::{Length, Sides};
use crate::dom::{Document, NodeId, NodeKind};
use crate::style::Style;

/// Wynik layoutu pojedynczego węzła: id DOM-owe i prostokąt border-boxu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LaidOutNode {
    pub node: NodeId,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Font wbudowany w binarny obraz (DejaVu Sans), do pomiaru tekstu.
pub fn assets_font_bytes() -> &'static [u8] {
    include_bytes!("../assets/font.ttf")
}

fn to_length_percentage(l: Length) -> LengthPercentage {
    match l {
        Length::Px(v) => LengthPercentage::length(v),
        // Procenty taffy są w zakresie 0..1, nasze 0..100.
        Length::Percent(p) => LengthPercentage::percent(p / 100.0),
    }
}

fn sides_to_rect_lp(sides: Sides<Length>) -> TaffyRect<LengthPercentage> {
    TaffyRect {
        left: to_length_percentage(sides.left),
        top: to_length_percentage(sides.top),
        right: to_length_percentage(sides.right),
        bottom: to_length_percentage(sides.bottom),
    }
}

fn sides_to_rect_lpa(sides: Sides<Length>) -> TaffyRect<LengthPercentageAuto> {
    let lp = sides_to_rect_lp(sides);
    TaffyRect {
        left: lp.left.into(),
        top: lp.top.into(),
        right: lp.right.into(),
        bottom: lp.bottom.into(),
    }
}

fn to_taffy_style(style: &Style) -> TaffyStyle {
    TaffyStyle {
        // Wyłącznie flexbox: każdy węzeł kontenerem flexa (feature `flexbox`
        // bez `block_layout`, więc `Display::Block` nie istnieje).
        display: Display::Flex,
        flex_direction: style.flex_direction,
        size: TaffySize {
            width: match style.width {
                Some(l) => to_length_percentage(l).into(),
                None => taffy::Dimension::AUTO,
            },
            height: match style.height {
                Some(l) => to_length_percentage(l).into(),
                None => taffy::Dimension::AUTO,
            },
        },
        padding: sides_to_rect_lp(style.padding),
        margin: sides_to_rect_lpa(style.margin),
        border: match style.border {
            Some(b) => TaffyRect {
                left: LengthPercentage::length(b.width),
                top: LengthPercentage::length(b.width),
                right: LengthPercentage::length(b.width),
                bottom: LengthPercentage::length(b.width),
            },
            None => TaffyRect {
                left: LengthPercentage::length(0.0),
                top: LengthPercentage::length(0.0),
                right: LengthPercentage::length(0.0),
                bottom: LengthPercentage::length(0.0),
            },
        },
        ..Default::default()
    }
}

fn ceil_f32(v: f32) -> f32 {
    // no_std: f32::ceil wymaga std; własny odpowiednik (NaN → 0).
    let t = v as i64 as f32;
    if v > t { t + 1.0 } else { t }
}

fn text_size(text: &str, font_size: f32, font: &fontdue::Font) -> TaffySize<f32> {
    let mut width = 0.0f32;
    for ch in text.chars() {
        width += font.metrics(ch, font_size).advance_width;
    }
    TaffySize { width, height: ceil_f32(font_size) }
}

/// Liczy layout poddrzewa od `root` w podanym viewportcie.
/// Zwraca węzły w pre-orderze. Złe `root` → pusty wynik; nic nie panikuje.
pub fn compute_layout(
    doc: &Document,
    styles: &[Style],
    root: NodeId,
    viewport: (f32, f32),
    font: &fontdue::Font,
) -> Vec<LaidOutNode> {
    let mut result = Vec::new();
    if root >= doc.nodes.len() {
        return result;
    }

    // Pre-order po arenie: jawny stos, maska odwiedzin, by każdy węzeł
    // przetworzyć raz (cykle i współdzielone dzieci odcina dopiero
    // `has_parent` przy dowiązywaniu dzieci w grafie taffy).
    let mut visited: Vec<bool> = alloc::vec![false; doc.nodes.len()];
    let mut order: Vec<NodeId> = Vec::new();
    let mut stack: Vec<NodeId> = Vec::new();
    stack.push(root);
    visited[root] = true;
    while let Some(id) = stack.pop() {
        order.push(id);
        let node = &doc.nodes[id];
        for &child in node.children.iter().rev() {
            if child < doc.nodes.len() && !visited[child] {
                visited[child] = true;
                stack.push(child);
            }
        }
    }

    let mut tree: TaffyTree<()> = TaffyTree::new();
    // taffy_id ↔ dom id; tworzymy wszystko jako liście, potem dowiązujemy
    // dzieci przez add_child — kolejność `dom_of_taffy` to pre-order.
    let mut dom_of_taffy: Vec<(TaffyId, NodeId)> = Vec::new();
    let mut text_of_taffy: Vec<(TaffyId, NodeId)> = Vec::new();
    let mut dom_to_taffy: Vec<Option<TaffyId>> = alloc::vec![None; doc.nodes.len()];
    let mut has_parent: Vec<bool> = alloc::vec![false; doc.nodes.len()];
    // Korzeń nie może zostać czyimś dzieckiem — to zrywa cykle w arenie
    // (A → B → A), zanim trafią do grafu taffy, w którym add_child nie
    // sprawdza rodzica, a layout rekurencyjnie z niej korzysta.
    has_parent[root] = true;
    let mut taffy_root = None;
    for &id in &order {
        let mut style = match styles.get(id) {
            Some(s) => s.clone(),
            None => crate::style::default_style(),
        };
        // Korzeń layoutu = initial containing block: bez jawnego rozmiaru
        // przyjmuje viewport (jak element html w przeglądarce).
        if taffy_root.is_none() {
            if style.width.is_none() {
                style.width = Some(Length::Px(viewport.0));
            }
            if style.height.is_none() {
                style.height = Some(Length::Px(viewport.1));
            }
        }
        let taffy_id = match tree.new_leaf(to_taffy_style(&style)) {
            Ok(t) => t,
            Err(_) => continue,
        };
        if taffy_root.is_none() {
            taffy_root = Some(taffy_id);
        }
        if matches!(doc.nodes[id].kind, NodeKind::Text(_)) {
            text_of_taffy.push((taffy_id, id));
        }
        dom_of_taffy.push((taffy_id, id));
        dom_to_taffy[id] = Some(taffy_id);
    }
    let Some(taffy_root) = taffy_root else { return result };

    for &(taffy_id, dom_id) in &dom_of_taffy {
        for &child in &doc.nodes[dom_id].children {
            if child >= dom_to_taffy.len() || has_parent[child] {
                continue;
            }
            if let Some(child_id) = dom_to_taffy[child] {
                if tree.add_child(taffy_id, child_id).is_ok() {
                    // Co najwyżej jeden rodzic w grafie taffy: pierwszy
                    // rodzic w pre-orderze wygrywa, kolejne dowiązania są
                    // pomijane (diament / współdzielone dziecko / cykl).
                    has_parent[child] = true;
                }
            }
        }
    }

    // Pomiar tekstu: fontdue mierzy w pikselach przy danym rozmiarze.
    let compute_ok = tree.compute_layout_with_measure(
        taffy_root,
        TaffySize {
            width: AvailableSpace::Definite(viewport.0),
            height: AvailableSpace::Definite(viewport.1),
        },
        |input: LayoutInput, taffy_id: TaffyId, _: Option<&mut ()>, style: &TaffyStyle| -> LayoutOutput {
            let text_info = text_of_taffy
                .iter()
                .find(|(t, _)| *t == taffy_id)
                .and_then(|(_, d)| match &doc.nodes[*d].kind {
                    NodeKind::Text(text) => Some((text.as_str(), styles.get(*d).map(|s| s.font_size).unwrap_or(16.0))),
                    _ => None,
                });
            // compute_leaf_layout zamyka styl (padding/border/rozmiar jawny);
            // mierzymy tylko treść, której styl nie określa.
            taffy::compute_leaf_layout(input, style, |_, _| 0.0, |known, _| match text_info {
                Some((text, font_size)) => {
                    let content = text_size(text, font_size, font);
                    known.unwrap_or(content)
                }
                None => known.unwrap_or(TaffySize { width: 0.0, height: 0.0 }),
            })
        },
    );
    if compute_ok.is_err() {
        return Vec::new();
    }

    for (taffy_id, dom_id) in dom_of_taffy {
        match tree.layout(taffy_id) {
            Ok(l) => result.push(LaidOutNode {
                node: dom_id,
                x: l.location.x,
                y: l.location.y,
                w: l.size.width,
                h: l.size.height,
            }),
            Err(_) => continue,
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
    use crate::css::{parse_stylesheet, Stylesheet};
    use crate::html::parse_html;
    use crate::style::style_tree;
    use fontdue::{Font, FontSettings};

    fn test_font() -> Font {
        Font::from_bytes(assets_font_bytes(), FontSettings::default()).unwrap()
    }

    #[test]
    fn fixed_column_layout() {
        let doc = parse_html("<div></div><div></div>");
        let sheet = parse_stylesheet("* { display: flex; flex-direction: column; } div { width: 100px; height: 50px; }");
        let styles = style_tree(&doc, &sheet, doc.root());
        let font = test_font();
        let out = compute_layout(&doc, &styles, doc.root(), (200.0, 200.0), &font);
        let d1 = doc.children(doc.root())[0];
        let d2 = doc.children(doc.root())[1];
        let l1 = out.iter().find(|l| l.node == d1).unwrap();
        let l2 = out.iter().find(|l| l.node == d2).unwrap();
        assert_eq!(l1.x, 0.0);
        assert_eq!(l1.y, 0.0);
        assert_eq!(l1.w, 100.0);
        assert_eq!(l1.h, 50.0);
        assert_eq!(l2.x, 0.0);
        assert_eq!(l2.y, 50.0);
        assert_eq!(l2.w, 100.0);
        assert_eq!(l2.h, 50.0);
    }

    #[test]
    fn percent_width_fills_viewport() {
        let doc = parse_html("<div></div>");
        let sheet = parse_stylesheet("div { width: 50%; height: 10px; }");
        let styles = style_tree(&doc, &sheet, doc.root());
        let font = test_font();
        let out = compute_layout(&doc, &styles, doc.root(), (200.0, 200.0), &font);
        let d = doc.children(doc.root())[0];
        let l = out.iter().find(|l| l.node == d).unwrap();
        assert_eq!(l.w, 100.0);
    }

    #[test]
    fn text_node_has_size() {
        let doc = parse_html("<span>hello</span>");
        let sheet = Stylesheet { rules: Vec::new() };
        let styles = style_tree(&doc, &sheet, doc.root());
        let font = test_font();
        let out = compute_layout(&doc, &styles, doc.root(), (200.0, 200.0), &font);
        let span = doc.children(doc.root())[0];
        let l = out.iter().find(|l| l.node == span).unwrap();
        assert!(l.w > 0.0, "span width should be positive, got {}", l.w);
        assert!(l.h > 0.0, "span height should be positive, got {}", l.h);
        assert_eq!(l.x, 0.0);
        assert_eq!(l.y, 0.0);
    }

    #[test]
    fn margin_offsets() {
        let doc = parse_html("<div></div>");
        let sheet = parse_stylesheet("div { margin: 10px; width: 50px; height: 20px; }");
        let styles = style_tree(&doc, &sheet, doc.root());
        let font = test_font();
        let out = compute_layout(&doc, &styles, doc.root(), (200.0, 200.0), &font);
        let d = doc.children(doc.root())[0];
        let l = out.iter().find(|l| l.node == d).unwrap();
        assert_eq!(l.x, 10.0);
        assert_eq!(l.y, 10.0);
        assert_eq!(l.w, 50.0);
        assert_eq!(l.h, 20.0);
    }

    #[test]
    fn root_out_of_bounds_returns_empty() {
        let doc = parse_html("<div></div>");
        let sheet = Stylesheet { rules: Vec::new() };
        let styles = style_tree(&doc, &sheet, doc.root());
        let font = test_font();
        let out = compute_layout(&doc, &styles, 999, (200.0, 200.0), &font);
        assert!(out.is_empty());
    }

    /// Arena z cyklem (A → B → A) — parser tego nie wytworzy, ale arenę można
    /// zbudować ręcznie. Layout nie może abortować (przepełnienie stosu w taffy
    /// to abort, nie błąd) i musi zwrócić deterministyczny wynik.
    #[test]
    fn cyclic_arena_does_not_crash() {
        use alloc::string::String;
        use crate::dom::{ElementData, Node};

        let mut doc = Document { nodes: Vec::new() };
        doc.nodes.push(Node {
            parent: None,
            children: alloc::vec![1],
            kind: NodeKind::Element(ElementData { tag: String::from("div"), attrs: Vec::new() }),
        });
        doc.nodes.push(Node {
            parent: Some(0),
            children: alloc::vec![0],
            kind: NodeKind::Element(ElementData { tag: String::from("span"), attrs: Vec::new() }),
        });

        let font = test_font();
        let out = compute_layout(&doc, &[], 0, (200.0, 200.0), &font);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].node, 0);
        assert_eq!(out[0].w, 200.0); // korzeń przyjmuje viewport
        assert_eq!(out[0].h, 200.0);
        assert_eq!(out[1].node, 1);
    }

    /// Węzeł będący własnym dzieckiem — ten sam klasa zagrożenia co cykl.
    #[test]
    fn self_loop_arena_does_not_crash() {
        use alloc::string::String;
        use crate::dom::{ElementData, Node};

        let mut doc = Document { nodes: Vec::new() };
        doc.nodes.push(Node {
            parent: None,
            children: alloc::vec![0],
            kind: NodeKind::Element(ElementData { tag: String::from("div"), attrs: Vec::new() }),
        });

        let font = test_font();
        let out = compute_layout(&doc, &[], 0, (200.0, 200.0), &font);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].node, 0);
        assert_eq!(out[0].w, 200.0);
    }
}
