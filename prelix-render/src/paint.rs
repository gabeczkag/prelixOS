//! Paint: DOM + style + layout → piksele. Tła, border i tekst na `tiny_skia::Pixmap`.
//!
//! Jedna odpowiedzialność: przyjmuje dokument, computed style i wynik layoutu,
//! zwraca pixmapę. Nigdy nie panikuje — każde rysowanie jest przycinane do
//! wymiarów pixmapy.

use tiny_skia::{Paint, Pixmap, Rect, Transform};

use crate::css::Color;
use crate::dom::{Document, NodeKind};
use crate::layout::LaidOutNode;
use crate::style::{Border, Style};

/// Rasteryzuje węzły w kolejności pre-orderu (tło → border → tekst).
pub fn paint(
    doc: &Document,
    styles: &[Style],
    laid_out: &[LaidOutNode],
    viewport: (u32, u32),
    font: &fontdue::Font,
) -> Pixmap {
    // `Pixmap::new` zwraca `None` tylko dla zerowych/przesadnych wymiarów;
    // fallback 1×1 jest więc zawsze osiągalny i nie zależy od wejścia.
    let mut pixmap = Pixmap::new(viewport.0.max(1), viewport.1.max(1))
        .or_else(|| Pixmap::new(1, 1))
        .expect("Pixmap 1x1 zawsze się tworzy");

    for l in laid_out {
        let Some(style) = styles.get(l.node) else { continue };

        fill_rect(&mut pixmap, l.x, l.y, l.w, l.h, style.background_color);

        if let Some(border) = &style.border {
            paint_border(&mut pixmap, l.x, l.y, l.w, l.h, border);
        }

        if let Some(NodeKind::Text(text)) = doc.nodes.get(l.node).map(|n| &n.kind) {
            paint_text(&mut pixmap, l, text, style, font);
        }
    }
    pixmap
}

/// Wypełnia prostokąt kolorem; `None` z `Rect::from_xywh` (niekończone/ujemne
/// współrzędne) po prostu nic nie rysuje. Clip do pixmapy robi tiny-skia.
fn fill_rect(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, color: Color) {
    if color.a == 0 || w <= 0.0 || h <= 0.0 {
        return;
    }
    if !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() {
        return;
    }
    let Some(rect) = Rect::from_xywh(x, y, w, h) else { return };
    let mut paint = Paint::default();
    paint.set_color_rgba8(color.r, color.g, color.b, color.a);
    paint.anti_alias = false;
    pixmap.fill_rect(rect, &paint, Transform::identity(), None);
}

/// Border jako cztery paski nałożone na krawędzie prostokąta (border-box):
/// piksel na krawędzi należy do borderu, środek do tła.
fn paint_border(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, border: &Border) {
    if !border.width.is_finite() || border.width <= 0.0 {
        return;
    }
    // Szerokość borderu nie może przekroczyć połowy boku, inaczej paski
    // wychodziłyby poza prostokąt albo miałyby ujemne rozmiary.
    let bw = border.width.min(w / 2.0).min(h / 2.0);
    if !(bw.is_finite()) || bw <= 0.0 {
        return;
    }
    fill_rect(pixmap, x, y, bw, h, border.color); // lewa
    fill_rect(pixmap, x + w - bw, y, bw, h, border.color); // prawa
    fill_rect(pixmap, x, y, w, bw, border.color); // górna
    fill_rect(pixmap, x, y + h - bw, w, bw, border.color); // dolna
}

/// Rasteryzuje znaki tekstu i blenduje glify alpha-over na pikselach pixmapy,
/// przycinając do prostokąta węzła i do wymiarów pixmapy.
fn paint_text(
    pixmap: &mut Pixmap,
    l: &LaidOutNode,
    text: &str,
    style: &Style,
    font: &fontdue::Font,
) {
    let (x, y, w, h) = (l.x, l.y, l.w, l.h);
    if style.color.a == 0 {
        return;
    }
    let font_size = style.font_size;
    if !font_size.is_finite() || font_size <= 0.0 {
        return;
    }
    if !x.is_finite() || !y.is_finite() || w <= 0.0 || h <= 0.0 {
        return;
    }

    // Linia bazowa: ascent z metryk fontu, awaryjnie 3/4 em.
    let ascent = font
        .horizontal_line_metrics(font_size)
        .map(|m| m.ascent)
        .unwrap_or(font_size * 0.75);
    let baseline = y + ascent;
    let right = x + w;
    let bottom = y + h;

    let (pw, ph) = (pixmap.width() as usize, pixmap.height() as usize);
    let pixels = pixmap.pixels_mut();
    let mut pen_x = x;

    for ch in text.chars() {
        let (m, bitmap) = font.rasterize(ch, font_size);
        // Bitmapa glifu zaczyna się w lewym górnym rogu; lewy górny piksel
        // bitmapy na ekranie to (pen_x + xmin, baseline - (ymin + height)),
        // bo xmin/ymin są w układzie y-up względem baseline.
        let gx = pen_x + m.xmin as f32;
        let gy = baseline - (m.ymin as f32 + m.height as f32);
        let (gw, gh) = (m.width, m.height);
        if bitmap.len() >= gw * gh {
            for row in 0..gh {
                let py_f = gy + row as f32;
                if py_f < y || py_f >= bottom {
                    continue;
                }
                let py = py_f as i32;
                if py < 0 || py as usize >= ph {
                    continue;
                }
                for col in 0..gw {
                    let cov = bitmap[row * gw + col] as u32;
                    if cov == 0 {
                        continue;
                    }
                    let px_f = gx + col as f32;
                    if px_f < x || px_f >= right {
                        continue;
                    }
                    let px = px_f as i32;
                    if px < 0 || px as usize >= pw {
                        continue;
                    }
                    // Alpha-over w przestrzeni premultiplied:
                    // src = kolor * (cov/255) * (alpha/255), dst skracane o (1 - sa/255).
                    let sa = (style.color.a as u32 * cov) / 255;
                    let ia = 255 - sa;
                    if sa == 0 {
                        continue;
                    }
                    let r = (style.color.r as u32 * cov) / 255
                        + (pixels[py as usize * pw + px as usize].red() as u32 * ia) / 255;
                    let g = (style.color.g as u32 * cov) / 255
                        + (pixels[py as usize * pw + px as usize].green() as u32 * ia) / 255;
                    let b = (style.color.b as u32 * cov) / 255
                        + (pixels[py as usize * pw + px as usize].blue() as u32 * ia) / 255;
                    let a = sa + (pixels[py as usize * pw + px as usize].alpha() as u32 * ia) / 255;
                    // Składowe ≤ alpha z konstrukcji, więc `from_rgba` zawsze zwraca Some;
                    // mimo to nie unwrapujemy.
                    if let Some(c) = tiny_skia::PremultipliedColorU8::from_rgba(
                        r.min(255) as u8,
                        g.min(255) as u8,
                        b.min(255) as u8,
                        a.min(255) as u8,
                    ) {
                        pixels[py as usize * pw + px as usize] = c;
                    }
                }
            }
        }
        pen_x += m.advance_width;
        if pen_x > right {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
    use crate::css::parse_stylesheet;
    use crate::html::parse_html;
    use crate::layout::{assets_font_bytes, compute_layout};
    use crate::style::style_tree;
    use fontdue::{Font, FontSettings};

    fn test_font() -> Font {
        Font::from_bytes(assets_font_bytes(), FontSettings::default()).unwrap()
    }

    #[test]
    fn background_pixel_is_painted() {
        let doc = parse_html("<div></div>");
        let sheet = parse_stylesheet("div { width: 100px; height: 50px; background-color: red; }");
        let styles = style_tree(&doc, &sheet, doc.root());
        let font = test_font();
        let laid_out = compute_layout(&doc, &styles, doc.root(), (200.0, 200.0), &font);
        let pix = paint(&doc, &styles, &laid_out, (200, 200), &font);
        let px = pix.pixel(50, 25).unwrap();
        assert_eq!(
            (px.red(), px.green(), px.blue(), px.alpha()),
            (255, 0, 0, 255),
            "środek diva powinien być czerwony"
        );
    }

    #[test]
    fn clips_outside_viewport() {
        let doc = parse_html("<div></div>");
        let sheet = parse_stylesheet(
            "div { margin: 0px 0px 0px 150px; width: 100px; height: 20px; background-color: green; }",
        );
        let styles = style_tree(&doc, &sheet, doc.root());
        let font = test_font();
        let laid_out = compute_layout(&doc, &styles, doc.root(), (200.0, 200.0), &font);
        let pix = paint(&doc, &styles, &laid_out, (200, 200), &font);
        assert_eq!(pix.width(), 200);
        assert_eq!(pix.height(), 200);
        let px = pix.pixel(199, 10).unwrap();
        assert!(px.alpha() > 0, "pixel (199,10) powinien być pomalowany");
    }

    #[test]
    fn text_pixels_appear() {
        let doc = parse_html("<div><span>A</span></div>");
        let sheet = parse_stylesheet(
            "div { width: 100px; height: 40px; background-color: white; } span { color: black; font-size: 24px; }",
        );
        let styles = style_tree(&doc, &sheet, doc.root());
        let font = test_font();
        let laid_out = compute_layout(&doc, &styles, doc.root(), (200.0, 200.0), &font);
        let pix = paint(&doc, &styles, &laid_out, (200, 200), &font);
        let div = doc.children(doc.root())[0];
        let span = doc.children(div)[0];
        let l = laid_out.iter().find(|l| l.node == span).unwrap();
        let x0 = l.x as u32;
        let y0 = l.y as u32;
        let x1 = (l.x + l.w).min(pix.width() as f32) as u32;
        let y1 = (l.y + l.h).min(pix.height() as f32) as u32;
        let found = (y0..y1).any(|y| {
            (x0..x1).any(|x| {
                pix.pixel(x, y)
                    .map(|px| px.alpha() > 0 && lum(px) < 128)
                    .unwrap_or(false)
            })
        });
        assert!(found, "w prostokącie spana powinien być co najmniej 1 ciemny pixel");
    }

    fn lum(px: tiny_skia::PremultipliedColorU8) -> u32 {
        let (r, g, b) = (px.red() as u32, px.green() as u32, px.blue() as u32);
        (r + g + b) / 3
    }

    #[test]
    fn border_is_drawn() {
        let doc = parse_html("<div></div>");
        let sheet = parse_stylesheet(
            "div { width: 100px; height: 50px; border: 4px solid black; background-color: red; }",
        );
        let styles = style_tree(&doc, &sheet, doc.root());
        let font = test_font();
        let laid_out = compute_layout(&doc, &styles, doc.root(), (200.0, 200.0), &font);
        let pix = paint(&doc, &styles, &laid_out, (200, 200), &font);
        let edge = pix.pixel(1, 1).unwrap();
        assert_eq!(
            (edge.red(), edge.green(), edge.blue()),
            (0, 0, 0),
            "krawędź powinna być czarna (border na pikselu (1,1))"
        );
        let center = pix.pixel(50, 25).unwrap();
        assert_eq!(
            (center.red(), center.green(), center.blue()),
            (255, 0, 0),
            "środek powinien być czerwony"
        );
    }
}
