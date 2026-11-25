//! Minimalny parser HTML -> arena DOM.
//!
//! Tokenizer znak-po-znaku: tag otwierający z atrybutami (`<tag attr="val">`),
//! tag zamykający (`</tag>`), tekst między tagami. Stos otwartych elementów;
//! EOF zamyka wszystko (węzły i tak są dopinane do rodzica od razu — stos służy
//! tylko do śledzenia otwartych elementów). `img`, `br`, `hr` to elementy void
//! (nie zbierają dzieci). Komentarze `<!--...-->` oraz `<!...>` (DOCTYPE) są
//! pomijane, jeśli wystąpią.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::dom::{Document, NodeId, NodeKind};

/// Elementy void: nigdy nie mają dzieci ani zamykającego tagu.
const VOID_TAGS: [&str; 3] = ["img", "br", "hr"];

pub fn parse_html(input: &str) -> Document {
    let mut doc = Document::new();
    let root = doc.push_element("html", Vec::new(), None);
    let mut stack: Vec<NodeId> = Vec::new();
    stack.push(root);

    let bytes = input.as_bytes();
    let mut text = String::new();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'<' {
            flush_text(&mut doc, &stack, &mut text);
            let next = if i + 1 < bytes.len() {
                Some(bytes[i + 1])
            } else {
                None
            };
            i = match next {
                // `<!--...-->` — komentarz, pomijamy do `-->`.
                Some(b'!') if input[i..].starts_with("<!--") => skip_comment(input, i),
                // `<!...>` — DOCTYPE i podobne, pomijamy do `>`.
                Some(b'!') => skip_to_tag_end(input, i),
                Some(b'/') => parse_closing(input, i, &doc, &mut stack),
                Some(c) if c.is_ascii_alphabetic() => {
                    parse_opening(input, i, &mut doc, &mut stack)
                }
                // Samotny `<` nie zaczynający tagu — zwykły tekst.
                _ => {
                    text.push('<');
                    i + 1
                }
            };
        } else {
            let ch = input[i..].chars().next().expect("valid utf-8 boundary");
            text.push(ch);
            i += ch.len_utf8();
        }
    }
    // EOF zamyka wszystko: nieprzeflushowany tekst ląduje w ostatnim otwartym
    // elemencie, a stos — już niepotrzebny — po prostu przestaje obowiązywać.
    flush_text(&mut doc, &stack, &mut text);
    doc
}

/// Dopisuje zakumulowany tekst jako węzeł Text pod ostatnim otwartym elementem.
/// Whitespace-only tekst jest pomijany; pozostały jest trimowany bez
/// zwijania wewnętrznych białych znaków.
fn flush_text(doc: &mut Document, stack: &[NodeId], text: &mut String) {
    if text.is_empty() {
        return;
    }
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        let parent = *stack.last().expect("stack never empty");
        doc.push_text(trimmed, Some(parent));
    }
    text.clear();
}

fn skip_comment(input: &str, start: usize) -> usize {
    match input[start + 4..].find("-->") {
        Some(end) => start + 4 + end + 3,
        None => input.len(),
    }
}

fn skip_to_tag_end(input: &str, start: usize) -> usize {
    match input[start..].find('>') {
        Some(end) => start + end + 1,
        None => input.len(),
    }
}

/// Zamykający tag: zdejmuje stos aż do pasującego elementu włącznie (razem
/// z nie zamkniętymi zagnieżdżeniami). Nieznany zamykający tag jest ignorowany.
/// Korzeń (indeks 0) nigdy nie jest zdejmowany — zamykający `</html>` jest
/// ignorowany, więc stos nigdy nie jest pusty i parser nie panikuje.
fn parse_closing(input: &str, start: usize, doc: &Document, stack: &mut Vec<NodeId>) -> usize {
    let inner = start + 2;
    match input[inner..].find('>') {
        Some(end) => {
            let name = input[inner..inner + end].trim();
            if let Some(pos) = stack.iter().rposition(|id| tag_of(doc, *id) == Some(name)) {
                stack.truncate(pos.max(1));
            }
            inner + end + 1
        }
        None => input.len(),
    }
}

fn tag_of(doc: &Document, id: NodeId) -> Option<&str> {
    match &doc.nodes[id].kind {
        NodeKind::Element(e) => Some(&e.tag),
        NodeKind::Text(_) => None,
    }
}

/// Otwierający tag z atrybutami. Zwraca pozycję za `>` (lub EOF).
fn parse_opening(input: &str, start: usize, doc: &mut Document, stack: &mut Vec<NodeId>) -> usize {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut j = start + 1;

    // Nazwa tagu: do whitespace, `>` albo `/`.
    let name_start = j;
    while j < len && !bytes[j].is_ascii_whitespace() && bytes[j] != b'>' && bytes[j] != b'/' {
        j += 1;
    }
    let tag = input[name_start..j].to_string();

    let mut attrs: Vec<(String, String)> = Vec::new();
    loop {
        while j < len && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= len {
            break;
        }
        match bytes[j] {
            b'>' => {
                j += 1;
                break;
            }
            // `/` w pozycji atrybutu pomijamy: void i tak nigdy nie wchodzi na
            // stos, a `/>` na nie-void elemencie go nie zamyka (HTML ignoruje
            // self-closing poza składnią XML).
            b'/' => {
                j += 1;
            }
            _ => {
                let attr_start = j;
                while j < len
                    && !bytes[j].is_ascii_whitespace()
                    && bytes[j] != b'='
                    && bytes[j] != b'>'
                    && bytes[j] != b'/'
                {
                    j += 1;
                }
                let name = input[attr_start..j].to_string();
                let mut value = String::new();

                // Opcjonalne `="value"` (cudzysłów, apostrof albo bez niczego).
                let mut k = j;
                while k < len && bytes[k].is_ascii_whitespace() {
                    k += 1;
                }
                if k < len && bytes[k] == b'=' {
                    k += 1;
                    while k < len && bytes[k].is_ascii_whitespace() {
                        k += 1;
                    }
                    if k < len && (bytes[k] == b'"' || bytes[k] == b'\'') {
                        let quote = bytes[k];
                        k += 1;
                        let value_start = k;
                        while k < len && bytes[k] != quote {
                            k += 1;
                        }
                        value = input[value_start..k].to_string();
                        if k < len {
                            k += 1;
                        }
                    } else {
                        let value_start = k;
                        while k < len && !bytes[k].is_ascii_whitespace() && bytes[k] != b'>' {
                            k += 1;
                        }
                        value = input[value_start..k].to_string();
                    }
                    j = k;
                }
                attrs.push((name, value));
            }
        }
    }

    let parent = *stack.last().expect("stack never empty");
    let id = doc.push_element(&tag, attrs, Some(parent));
    if !VOID_TAGS.contains(&tag.as_str()) {
        stack.push(id);
    }
    j
}

#[cfg(test)]
mod tests {
    use super::parse_html;
    use crate::dom::{Document, ElementData, NodeId, NodeKind};
    use alloc::string::String;
    use alloc::vec::Vec;

    fn element(tag: &str) -> NodeKind {
        NodeKind::Element(ElementData {
            tag: String::from(tag),
            attrs: Vec::new(),
        })
    }

    fn text(s: &str) -> NodeKind {
        NodeKind::Text(String::from(s))
    }

    fn kind(doc: &Document, id: NodeId) -> &NodeKind {
        &doc.nodes[id].kind
    }

    #[test]
    fn parses_nested_elements() {
        let doc = parse_html("<div><span>hi</span></div>");
        let root = doc.root();
        assert_eq!(doc.children(root).len(), 1);
        let div = doc.children(root)[0];
        assert_eq!(kind(&doc, div), &element("div"));
        assert_eq!(doc.children(div).len(), 1);
        let span = doc.children(div)[0];
        assert_eq!(kind(&doc, span), &element("span"));
        assert_eq!(doc.children(span).len(), 1);
        assert_eq!(kind(&doc, doc.children(span)[0]), &text("hi"));
    }

    #[test]
    fn parses_attributes() {
        let doc = parse_html("<div class=\"box\" id=\"a\">x</div>");
        let div = doc.children(doc.root())[0];
        match kind(&doc, div) {
            NodeKind::Element(e) => {
                assert!(e
                    .attrs
                    .contains(&(String::from("class"), String::from("box"))));
                assert!(e.attrs.contains(&(String::from("id"), String::from("a"))));
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn auto_closes_unclosed_tags() {
        let doc = parse_html("<div><span>tekst");
        let root = doc.root();
        assert_eq!(doc.children(root).len(), 1);
        let div = doc.children(root)[0];
        assert_eq!(doc.children(div).len(), 1);
        let span = doc.children(div)[0];
        assert_eq!(kind(&doc, span), &element("span"));
        assert_eq!(doc.children(span).len(), 1);
        assert_eq!(kind(&doc, doc.children(span)[0]), &text("tekst"));
    }

    #[test]
    fn treats_img_as_void() {
        let doc = parse_html("<div><img src=\"x.png\"></div>");
        let div = doc.children(doc.root())[0];
        assert_eq!(doc.children(div).len(), 1);
        let img = doc.children(div)[0];
        match kind(&doc, img) {
            NodeKind::Element(e) => {
                assert_eq!(e.tag, "img");
                assert!(e.attrs.contains(&(String::from("src"), String::from("x.png"))));
            }
            other => panic!("expected element, got {other:?}"),
        }
        assert_eq!(doc.children(img).len(), 0);

        // Tag po img nie staje się dzieckiem img — rodzeństwem wewnątrz div.
        let doc = parse_html("<div><img src=\"x.png\"><b>y</b></div>");
        let div = doc.children(doc.root())[0];
        assert_eq!(doc.children(div).len(), 2);
        let b = doc.children(div)[1];
        assert_eq!(kind(&doc, b), &element("b"));
        assert_eq!(doc.children(b).len(), 1);
        assert_eq!(kind(&doc, doc.children(b)[0]), &text("y"));
    }

    #[test]
    fn whitespace_only_text_is_skipped() {
        let doc = parse_html("<div>   \n\t  </div>");
        let div = doc.children(doc.root())[0];
        assert_eq!(doc.children(div).len(), 0);

        // Whitespace między tagami na poziomie korzenia też nie tworzy węzłów.
        let doc = parse_html("  <div>a</div>\n  <div>b</div>  ");
        assert_eq!(doc.children(doc.root()).len(), 2);
    }

    #[test]
    fn closing_root_tag_does_not_panic() {
        // `</html>` na wejściu: korzeń nie może być zdjęty ze stosu.
        let doc = parse_html("</html>x");
        let root = doc.root();
        assert_eq!(doc.children(root).len(), 1);
        assert_eq!(kind(&doc, doc.children(root)[0]), &text("x"));
    }

    #[test]
    fn closing_root_tag_mid_document_keeps_siblings() {
        let doc = parse_html("<div></html><span>y</span></div>");
        let root = doc.root();
        assert_eq!(doc.children(root).len(), 2);
        let div = doc.children(root)[0];
        assert_eq!(kind(&doc, div), &element("div"));
        assert_eq!(doc.children(div).len(), 0);
        let span = doc.children(root)[1];
        assert_eq!(kind(&doc, span), &element("span"));
        assert_eq!(doc.children(span).len(), 1);
        assert_eq!(kind(&doc, doc.children(span)[0]), &text("y"));
    }

    #[test]
    fn self_closing_slash_keeps_non_void_open() {
        // `/>` na nie-void elemencie nie zamyka go: span zostaje dzieckiem div.
        let doc = parse_html("<div/><span>x</span>");
        let root = doc.root();
        assert_eq!(doc.children(root).len(), 1);
        let div = doc.children(root)[0];
        assert_eq!(kind(&doc, div), &element("div"));
        assert_eq!(doc.children(div).len(), 1);
        let span = doc.children(div)[0];
        assert_eq!(kind(&doc, span), &element("span"));
        assert_eq!(doc.children(span).len(), 1);
        assert_eq!(kind(&doc, doc.children(span)[0]), &text("x"));
    }
}
