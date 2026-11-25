//! Parser CSS (podzbiór): tekst → `Stylesheet`. Nigdy nie panikuje na wejściu:
//! nieznane/zepsute deklaracje i bloki są pomijane.

use alloc::string::String;
use alloc::vec::Vec;

pub type FlexDirection = taffy::FlexDirection;

#[derive(Debug, Clone, PartialEq)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub selector: Selector,
    pub declarations: Vec<Declaration>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    Universal,
    Tag(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Declaration {
    BackgroundColor(Color),
    Color(Color),
    FontSize(f32),
    Width(Length),
    Height(Length),
    Padding(Sides<Length>),
    Margin(Sides<Length>),
    Border { width: f32, color: Color },
    DisplayFlex,
    FlexDirection(FlexDirection),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    Px(f32),
    Percent(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sides<T> {
    pub left: T,
    pub top: T,
    pub right: T,
    pub bottom: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

fn parse_selector(text: &str) -> Option<Selector> {
    // Tolerancja na śmieci przed `{`: bierzemy wszystko po ostatnim `}`.
    let text = text.rsplit('}').next().unwrap_or(text).trim();
    if text.is_empty() {
        return None;
    }
    if text == "*" {
        return Some(Selector::Universal);
    }
    Some(Selector::Tag(text.to_ascii_lowercase()))
}

fn parse_declaration(name: &str, value: &str) -> Option<Declaration> {
    let name = name.trim().to_ascii_lowercase();
    let value = value.trim();
    match name.as_str() {
        "background-color" => parse_color(value).map(Declaration::BackgroundColor),
        "color" => parse_color(value).map(Declaration::Color),
        "font-size" => parse_number_px(value).map(Declaration::FontSize),
        "width" => parse_length(value).map(Declaration::Width),
        "height" => parse_length(value).map(Declaration::Height),
        "padding" => parse_sides(value).map(Declaration::Padding),
        "margin" => parse_sides(value).map(Declaration::Margin),
        "border" => parse_border(value),
        "display" if value == "flex" => Some(Declaration::DisplayFlex),
        "flex-direction" => parse_flex_direction(value),
        _ => None,
    }
}

fn parse_color(value: &str) -> Option<Color> {
    if let Some(hex) = value.strip_prefix('#') {
        return parse_hex_color(hex);
    }
    let c = match value.to_ascii_lowercase().as_str() {
        "white" => (255, 255, 255),
        "black" => (0, 0, 0),
        "red" => (255, 0, 0),
        "green" => (0, 128, 0),
        "blue" => (0, 0, 255),
        "gray" => (128, 128, 128),
        _ => return None,
    };
    Some(Color { r: c.0, g: c.1, b: c.2, a: 255 })
}

fn parse_hex_color(hex: &str) -> Option<Color> {
    // Slicing po indeksach jest bezpieczny dopiero po sprawdzeniu,
    // że to dokładnie 6 bajtów ASCII.
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some(Color { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: 255 })
}

fn parse_length(value: &str) -> Option<Length> {
    if let Some(n) = value.strip_suffix("px") {
        return n.trim().parse::<f32>().ok().map(Length::Px);
    }
    if let Some(n) = value.strip_suffix('%') {
        return n.trim().parse::<f32>().ok().map(Length::Percent);
    }
    None
}

fn parse_number_px(value: &str) -> Option<f32> {
    let n = value.strip_suffix("px").unwrap_or(value).trim();
    n.parse::<f32>().ok()
}

/// Skrót `padding`/`margin` jak w CSS: 1 wartość = 4 strony,
/// 2 = (pion, poziom), 3 = (top, poziom, bottom), 4 = (top, right, bottom, left).
fn parse_sides(value: &str) -> Option<Sides<Length>> {
    let values: Vec<Length> = value.split_whitespace().filter_map(parse_length).collect();
    if values.len() != value.split_whitespace().count() {
        return None; // jakaś wartość była nieparowalna → deklaracja ignorowana
    }
    match values.as_slice() {
        [v] => Some(Sides { top: *v, right: *v, bottom: *v, left: *v }),
        [v, h] => Some(Sides { top: *v, right: *h, bottom: *v, left: *h }),
        [t, h, b] => Some(Sides { top: *t, right: *h, bottom: *b, left: *h }),
        [t, r, b, l] => Some(Sides { top: *t, right: *r, bottom: *b, left: *l }),
        _ => None,
    }
}

fn parse_border(value: &str) -> Option<Declaration> {
    let mut tokens = value.split_whitespace();
    let width = parse_number_px(tokens.next()?)?;
    let color = tokens.next_back().and_then(parse_color)?;
    Some(Declaration::Border { width, color })
}

fn parse_flex_direction(value: &str) -> Option<Declaration> {
    let dir = match value.to_ascii_lowercase().as_str() {
        "row" => FlexDirection::Row,
        "column" => FlexDirection::Column,
        _ => return None,
    };
    Some(Declaration::FlexDirection(dir))
}

pub fn parse_stylesheet(input: &str) -> Stylesheet {
    let mut rules = Vec::new();
    let mut rest = input;
    while let Some(brace) = rest.find('{') {
        let selector = parse_selector(&rest[..brace]);
        rest = &rest[brace + 1..];
        let end = rest.find('}').unwrap_or(rest.len());
        let body = &rest[..end];
        rest = rest.get(end + 1..).unwrap_or("");

        let Some(selector) = selector else { continue };
        let declarations = body
            .split(';')
            .filter_map(|decl| decl.split_once(':'))
            .filter_map(|(name, value)| parse_declaration(name, value))
            .collect();
        rules.push(Rule { selector, declarations });
    }
    Stylesheet { rules }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rule_with_tag_selector() {
        let css = parse_stylesheet("div { background-color: #ff0000; }");
        assert_eq!(css.rules.len(), 1);
        assert_eq!(css.rules[0].selector, Selector::Tag(String::from("div")));
        assert!(matches!(
            css.rules[0].declarations[0],
            Declaration::BackgroundColor(Color { r: 255, .. })
        ));
    }

    #[test]
    fn parses_named_color() {
        let css = parse_stylesheet("* { color: white; }");
        assert_eq!(css.rules.len(), 1);
        assert_eq!(css.rules[0].selector, Selector::Universal);
        assert!(matches!(
            css.rules[0].declarations[0],
            Declaration::Color(Color { r: 255, g: 255, b: 255, .. })
        ));
    }

    #[test]
    fn parses_lengths() {
        let css = parse_stylesheet("div { width: 100px; padding: 10px 20%; }");
        assert!(matches!(
            css.rules[0].declarations[0],
            Declaration::Width(Length::Px(100.0))
        ));
        assert!(matches!(
            css.rules[0].declarations[1],
            Declaration::Padding(Sides {
                left: Length::Percent(20.0),
                top: Length::Px(10.0),
                right: Length::Percent(20.0),
                bottom: Length::Px(10.0),
            })
        ));
    }

    #[test]
    fn unknown_declarations_ignored() {
        let css = parse_stylesheet("div { rotate: 3px; background-color: gray; }");
        assert_eq!(css.rules.len(), 1);
        assert_eq!(css.rules[0].declarations.len(), 1);
        assert!(matches!(
            css.rules[0].declarations[0],
            Declaration::BackgroundColor(Color { r: 128, g: 128, b: 128, .. })
        ));
    }

    #[test]
    fn multiple_rules() {
        let css = parse_stylesheet("div { color: red }\n  p { width: 50px }");
        assert_eq!(css.rules.len(), 2);
        assert_eq!(css.rules[0].selector, Selector::Tag(String::from("div")));
        assert_eq!(css.rules[1].selector, Selector::Tag(String::from("p")));
    }
}
