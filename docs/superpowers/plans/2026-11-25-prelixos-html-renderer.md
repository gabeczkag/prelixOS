# prelixOS HTML Renderer — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Minimalny OS w Ruście, który renderuje HTML (podzbiór UI + podzbiór CSS) na framebuffer; ten sam renderer działa na hoście i zapisuje PNG.

**Architecture:** Cargo workspace z 4 crate'ami: `prelix-render` (rdzeń `no_std + alloc`: parser HTML → DOM arena, parser CSS, layout na `taffy`, rasteryzacja `tiny-skia` + `fontdue`), `host` (bin std: HTML → PNG), `kernel` (bin `no_std` na `bootloader_api` 0.11, sterta, blit bufora na framebuffer), `os` (bin host: build.rs skleja obraz dysku, uruchamia QEMU).

**Tech Stack:** Rust nightly, `bootloader_api` 0.11 / `bootloader` 0.11, `linked_list_allocator` 0.10, `taffy` 0.16 (feature `alloc`), `tiny-skia` 0.12 (feature `no-std-float`), `fontdue` 0.9, `image` 0.25 (tylko host), QEMU.

**Spec:** `docs/superpowers/specs/2026-11-25-prelixos-html-renderer-design.md`

## Global Constraints

- Rdzeń `prelix-render` musi się kompilować jako `#![no_std]` z `extern crate alloc;` — zero zależności od `std` w tym crate.
- Wersje: `bootloader_api = "0.11"`, `bootloader = "0.11"`, `linked_list_allocator = "0.10"`, `taffy = { version = "0.16", default-features = false, features = ["alloc"] }`, `tiny-skia = { version = "0.12", default-features = false, features = ["no-std-float"] }`, `fontdue = "0.9"` (jeśli domyślne features ciągną `std`, przełącz na `default-features = false, features = ["hashbrown"]`), `image = "0.25"` (tylko `host`).
- Toolchain: nightly + `rust-src` + `llvm-tools-preview` + target `x86_64-unknown-none` (zapis w `rust-toolchain.toml`).
- Obsługiwany podzbiór HTML: tagi `div`, `span`, `img` (void), tekst. Nieznane tagi trafiają do drzewa jako zwykłe elementy — parser nigdy nie panikuje na wejściu.
- Obsługiwany podzbiór CSS: selektory `*` i nazwa tagu; deklaracje `background-color`, `color`, `width`, `height`, `padding`, `margin`, `border`, `font-size`, `display: flex`, `flex-direction: row|column`; jednostki `px` i `%`; kolory `#rrggbb` oraz nazwy `white black red green blue gray`. Nieznane deklaracje są ignorowane.
- Kryterium sukcesu: ten sam kod `prelix-render` daje identyczny bufor pikseli na hoście (PNG) i w QEMU (framebuffer).

## Review Focus

1. Niezamknięte tagi (`<div><span>tekst`) — parser ma zamknąć wszystko na EOF, bez paniki; test w Task 1.
2. Pusta strona / pusty CSS — `render("")` ma dać pusty pixmap w tle, nie błąd; test w Task 6.
3. Elementy wystające poza ekran i ujemne współrzędne — paint ma przycinać do bufora, nie rysować poza; test w Task 5.
4. Niepoprawne bajty fontu — `Font::from_bytes` zwraca `Result`; host i kernel mają czytelny panic z komunikatem, nie corrupt memory; test w Task 6 (host).
5. Framebuffer o innych niż 32 bpp/others — blit ma czytać `stride` i `bytes_per_pixel` z `boot_info`, nie zakładać 4; weryfikacja w QEMU w Task 7.

---

### Task 0: Workspace i toolchain

**Files:**
- Create: `rust-toolchain.toml`, `Cargo.toml`, `.cargo/config.toml`, `.gitignore`
- Create: `prelix-render/Cargo.toml`, `prelix-render/src/lib.rs`
- Create: `host/Cargo.toml`, `host/src/main.rs`
- Create: `kernel/Cargo.toml`, `kernel/src/main.rs` (szkielet no_std)
- Create: `os/Cargo.toml`, `os/src/main.rs` (szkielet)

**Interfaces:**
- Produces: workspace `prelixOS` z crate'ami `prelix-render` (lib), `host` (bin), `kernel` (bin), `os` (bin).

- [ ] **Step 1: Zainstaluj/sprawdź toolchain**

Run: `rustup toolchain install nightly && rustup component add rust-src llvm-tools-preview --toolchain nightly && rustup target add x86_64-unknown-none --toolchain nightly && which qemu-system-x86_64`
Expected: komponenty zainstalowane; jeśli QEMU brak — `sudo apt install qemu-system-x86`.

- [ ] **Step 2: Utwórz pliki workspace**

`rust-toolchain.toml`:
```toml
[toolchain]
channel = "nightly"
targets = ["x86_64-unknown-none"]
components = ["rust-src", "llvm-tools-preview"]
```

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["prelix-render", "host", "kernel", "os"]
```

`.cargo/config.toml`:
```toml
[unstable]
bindeps = true
```

`.gitignore`: `target/`.

`prelix-render/Cargo.toml`:
```toml
[package]
name = "prelix-render"
version = "0.1.0"
edition = "2021"

[dependencies]
taffy = { version = "0.16", default-features = false, features = ["alloc"] }
tiny-skia = { version = "0.12", default-features = false, features = ["no-std-float"] }
fontdue = { version = "0.9", default-features = false, features = ["hashbrown"] }
```

`prelix-render/src/lib.rs`:
```rust
#![no_std]
extern crate alloc;
```

`kernel/Cargo.toml`:
```toml
[package]
name = "kernel"
version = "0.1.0"
edition = "2021"

[dependencies]
bootloader_api = "0.11"
linked_list_allocator = "0.10"
prelix-render = { path = "../prelix-render" }
```

`kernel/src/main.rs` (na teraz minimalny, żeby się kompilował):
```rust
#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    loop {}
}
```

`host/Cargo.toml`:
```toml
[package]
name = "host"
version = "0.1.0"
edition = "2021"

[dependencies]
prelix-render = { path = "../prelix-render" }
image = "0.25"
```

`host/src/main.rs`: `fn main() { println!("host ok"); }`

`os/Cargo.toml`:
```toml
[package]
name = "os"
version = "0.1.0"
edition = "2021"

[dependencies]
bootloader = "0.11"

[build-dependencies]
bootloader = "0.11"
kernel = { path = "../kernel", artifact = "bin", target = "x86_64-unknown-none" }
```

`os/src/main.rs`: na razie `fn main() { println!("os ok"); }`, bez build.rs (pojawi się w Task 7).

- [ ] **Step 3: Zainicjuj git i zweryfikuj build hosta**

Run: `git init && git add -A && git commit -m "chore: workspace scaffold" && cargo build -p host`
Expected: kompilacja OK.

- [ ] **Step 4: Zweryfikuj build kernela**

Run: `cargo build -p kernel --target x86_64-unknown-none`
Expected: kompilacja OK na bare-metal (dowód, że rdzeń zależności jest no_std-clean; taffy/tiny-skia/fontdue jeszcze nieużywane).

### Task 1: DOM i parser HTML

**Files:**
- Create: `prelix-render/src/dom.rs`, `prelix-render/src/html.rs`
- Modify: `prelix-render/src/lib.rs` (dodać `mod dom; mod html;` + `pub use`)
- Test: `prelix-render/src/html.rs` (mod testy)

**Interfaces:**
- Produces:
```rust
pub type NodeId = usize;
pub struct Document { pub nodes: Vec<Node> }
pub struct Node { pub parent: Option<NodeId>, pub children: Vec<NodeId>, pub kind: NodeKind }
pub enum NodeKind { Element(ElementData), Text(String) }
pub struct ElementData { pub tag: String, pub attrs: Vec<(String, String)> }
pub fn parse_html(input: &str) -> Document   // korzeń: node tagu "html"
pub impl Document { pub fn root(&self) -> NodeId; pub fn children(&self, id: NodeId) -> &[NodeId] }
```

- [ ] **Step 1: Napisz testy (failing)**

Testy w `prelix-render/src/html.rs`:
- `parses_nested_elements`: `"<div><span>hi</span></div>"` → 1 element div z dzieckiem span, span ma dziecko Text `"hi"`.
- `parses_attributes`: `"<div class="box" id="a">x</div>"` → attrs zawiera `("class","box")` i `("id","a")`.
- `auto_closes_unclosed_tags`: `"<div><span>tekst"` → drzewo kompletne, span zamknięty na EOF, tekst zapadniętym whitespace-trimmed.
- `treats_img_as_void`: `"<div><img src="x.png"></div>"` → img NIE ma dziecka następującego po nim; kolejny tag nie zostaje jego potomkiem.
- `whitespace_only_text_is_skipped`: między tagami same spacje/newline nie tworzą node'ów Text.

- [ ] **Step 2: Uruchom, sprawdź że nie przechodzą**

Run: `cargo test -p prelix-render`
Expected: FAIL (brak `parse_html`).

- [ ] **Step 3: Zaimplementuj parser**

Tokenizer znak-po-znaku: `<tag attr="val">`, `</tag>`, tekst między tagami. Stos otwartych elementów; EOF zamyka wszystko. `img`, `br`, `hr` traktuje jako void. Bez obsługi komentarzy i DOCTYPE (pomijane, jeśli wystąpią — dopisz tylko tyle, by nie zaczynać nazwy tagu od `!`).

- [ ] **Step 4: Testy przechodzą**

Run: `cargo test -p prelix-render`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add prelix-render/src && git commit -m "feat(render): HTML parser producing arena DOM"
```

### Task 2: Parser CSS (podzbiór)

**Files:**
- Create: `prelix-render/src/css.rs`
- Modify: `prelix-render/src/lib.rs` (`mod css; pub use css::*;`)

**Interfaces:**
- Produces:
```rust
pub struct Stylesheet { pub rules: Vec<Rule> }
pub struct Rule { pub selector: Selector, pub declarations: Vec<Declaration> }
pub enum Selector { Universal, Tag(String) }
pub enum Declaration {
    BackgroundColor(Color), Color(Color), FontSize(f32),
    Width(Length), Height(Length),
    Padding(Sides<Length>), Margin(Sides<Length>),
    Border { width: f32, color: Color },
    DisplayFlex, FlexDirection(FlexDirection),
}
pub enum Length { Px(f32), Percent(f32) }
pub struct Sides<T> { pub left: T, pub top: T, pub right: T, pub bottom: T }
pub struct Color { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }
pub type FlexDirection = taffy::FlexDirection;  // alias, patrz Task 4
pub fn parse_stylesheet(input: &str) -> Stylesheet
```

- [ ] **Step 1: Napisz testy (failing)**

- `parses_rule_with_tag_selector`: `"div { background-color: #ff0000; }"` → 1 reguła, selector Tag("div"), BackgroundColor z r=255.
- `parses_named_color`: `"* { color: white; }"` → Color r/g/b=255.
- `parses_lengths`: `"div { width: 100px; padding: 10px 20%; }"` → Width Px(100), padding left/right Percent(20), top/bottom Px(10).
- `unknown_declarations_ignored`: `"div { rotate: 3px; background-color: gray; }"` → tylko BackgroundColor.
- `multiple_rules`: dwa bloki `{}` → Stylesheet z 2 regułami; białe znaki i brak średnika po ostatniej deklaracji są ok.

- [ ] **Step 2: Uruchom, sprawdź że nie przechodzą**

Run: `cargo test -p prelix-render`
Expected: FAIL.

- [ ] **Step 3: Zaimplementuj parser**

Podział na bloki po `{`/`}`, selector = trim przed `{` (universal `*` albo lowercase nazwa tagu), deklaracje po `;`, `name: value`. Rozbicie `padding: a b` jak w CSS (1 wartość = 4 strony, 2 = (pion, poziom)). `parse_declaration(name, value) -> Option<Declaration>`; nieznane → `None`.

- [ ] **Step 4: Testy przechodzą**

Run: `cargo test -p prelix-render`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add prelix-render/src && git commit -m "feat(render): CSS subset parser"
```

### Task 3: Stylowanie DOM (computed style)

**Files:**
- Create: `prelix-render/src/style.rs`
- Modify: `prelix-render/src/lib.rs` (`mod style; pub use style::*;`)

**Interfaces:**
- Consumes: `Document`, `Stylesheet` z Tasks 1–2.
- Produces:
```rust
pub struct Style {
    pub background_color: Color, pub color: Color, pub font_size: f32,
    pub display_flex: bool, pub flex_direction: FlexDirection,
    pub width: Option<Length>, pub height: Option<Length>,
    pub padding: Sides<Length>, pub margin: Sides<Length>,
    pub border: Option<Border>,
}
pub struct Border { pub width: f32, pub color: Color }
pub const DEFAULT_STYLE: fn() -> Style   // tło przezroczyste, kolor tekstu czarny, font_size 16.0, padding/margin Px(0)
pub fn style_tree(doc: &Document, sheet: &Stylesheet, root: NodeId) -> Vec<Style>  // indeks = NodeId
```

- [ ] **Step 1: Napisz testy (failing)**

- `applies_matching_tag_rules`: HTML `"<div></div><span></span>"` + CSS `"div { background-color: red; }"` → style[div].background_color czerwony, style[span] domyślny.
- `universal_rule_hits_everything`: `"* { color: blue; }"` → oba node'y mają niebieski kolor.
- `color_and_font_size_inherit`: CSS `"div { color: red; font-size: 32px; }"`, HTML `"<div>text</div>"` → dziecko tekstowe ma odziedziczone `color: red`, `font_size: 32.0` (dziedziczone: `color`, `font_size`; reszta nie).
- `later_rule_wins`: `"div { color: red; } div { color: blue; }"` → niebieski.

- [ ] **Step 2: Uruchom, sprawdź że nie przechodzą**

Run: `cargo test -p prelix-render`
Expected: FAIL.

- [ ] **Step 3: Zaimplementuj style_tree**

Pre-order przejście drzewa; dla każdego elementu zbierz deklaracje z reguł, których selector pasuje (Tag == lowercase tag, Universal zawsze), w kolejności arkusza. `color`/`font_size` dziedziczone od rodzica, jeśli nie nadpisane; węzłom Text — tylko dziedziczenie.

- [ ] **Step 4: Testy przechodzą**

Run: `cargo test -p prelix-render`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add prelix-render/src && git commit -m "feat(render): computed styles with inheritance"
```

### Task 4: Layout na taffy

**Files:**
- Create: `prelix-render/src/layout.rs`
- Modify: `prelix-render/src/lib.rs` (`mod layout; pub use layout::*;`)
- Modify: `prelix-render/src/css.rs` — jeśli `FlexDirection` zdefiniowano lokalnie w Task 2, podmień alias na `taffy::FlexDirection` i dostosuj parser (wartości `row`/`column`).

**Interfaces:**
- Consumes: `Document`, `Stylesheet`, `Vec<Style>`, `fontdue::Font` (do pomiaru tekstu).
- Produces:
```rust
pub struct LaidOutNode { pub node: NodeId, pub x: f32, pub y: f32, pub w: f32, pub h: f32 }
pub fn compute_layout(doc: &Document, styles: &[Style], root: NodeId,
    viewport: (f32, f32), font: &fontdue::Font) -> Vec<LaidOutNode>  // pre-order
```

- [ ] **Step 1: Napisz testy (failing)**

Testy używają `Font::from_bytes(assets_font_bytes(), FontSettings::default()).unwrap()` (helper czyta `prelix-render/assets/font.ttf`, patrz Step 3).

- `fixed_column_layout`: HTML `"<div></div><div></div>"`, CSS `"* { display: flex; flex-direction: column; } div { width: 100px; height: 50px; }"`, viewport (200, 200) → oba prostokąty x=0, w=100, h=50; pierwszy y=0, drugi y=50.
- `percent_width_fills_viewport`: `"<div></div>"`, `"div { width: 50%; height: 10px; }"` → w == 100 przy viewport 200.
- `text_node_has_size`: HTML `"<span>hello</span>"`, viewport (200, 200) → node span ma w > 0 i h > 0 (zmierzone przez fontdue), x=0, y=0.
- `margin_offsets`: `"<div></div>"`, `"div { margin: 10px; width: 50px; height: 20px; }"` → x=10, y=10, w=50, h=20.

- [ ] **Step 2: Uruchom, sprawdź że nie przechodzą**

Run: `cargo test -p prelix-render`
Expected: FAIL.

- [ ] **Step 3: Zbuduj drzewo taffy i policz layout**

Zbuduj `taffy::TaffyTree` z node'ów DOM: style taffy z `Style` (width/height przez `taffy::Size::length`/`percent`, padding/margin przez `taffy::Rect`, `display: Flex` gdy `display_flex`). Liście tekstowe: `set_measure` zwraca `taffy::Size` z sumy szerokości glifów (`font.horizontal_advance` dla znaków przy `font_size`) i wysokości `font_size` zaokrąglonej w górę do `line_height`. `compute_layout(root_node, Size { width: Defined, height: Defined })`, potem odczytaj `tree.layout(node)` i zwróć `LaidOutNode` w pre-order. Dodaj katalog `prelix-render/assets/` i skopiuj font: `cp /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf prelix-render/assets/font.ttf` (helper `pub fn assets_font_bytes() -> &'static [u8]` = `include_bytes!("../assets/font.ttf")`).

- [ ] **Step 4: Testy przechodzą**

Run: `cargo test -p prelix-render`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add prelix-render && git commit -m "feat(render): taffy flexbox layout of DOM"
```

### Task 5: Paint (prostokąty, border, tekst)

**Files:**
- Create: `prelix-render/src/paint.rs`
- Modify: `prelix-render/src/lib.rs` (`mod paint; pub use paint::*;`)

**Interfaces:**
- Consumes: `Document`, `Vec<Style>`, `Vec<LaidOutNode>`, `fontdue::Font`.
- Produces:
```rust
pub fn paint(doc: &Document, styles: &[Style], laid_out: &[LaidOutNode],
    viewport: (u32, u32), font: &fontdue::Font) -> tiny_skia::Pixmap
```

- [ ] **Step 1: Napisz testy (failing)**

- `background_pixel_is_painted`: div 100×50 czerwone tło na (0,0), viewport (200,200) → pixel (50,25) == rgba(255,0,0,255).
- `clips_outside_viewport`: div na x=150, w=100 przy viewport 200 → pixel (199,10) pomalowany, rysowanie nie panikuje i nic poza buforem.
- `text_pixels_appear`: span z tekstem `"A"` w stylu color czarny na białym tle rodzica (rodzic `background-color: white`) → w prostokącie spana istnieje co najmniej 1 pixel z alpha > 0 i jasnością < 128.
- `border_is_drawn`: div z `border: 4px solid black` i tłem czerwonym → pixel na krawędzi (x+1, y+1) czarny, pixel w środku czerwony.

- [ ] **Step 2: Uruchom, sprawdź że nie przechodzą**

Run: `cargo test -p prelix-render`
Expected: FAIL.

- [ ] **Step 3: Zaimplementuj paint**

`Pixmap::new(w, h)` (tło przezroczyste), dla każdego `LaidOutNode` w kolejności: `fill_rect` (tiny-skia `Rect::from_xywh`, `Paint { style: Fill }`) dla `background_color`, obrys 4 pasków dla `border`, tekst: iteruj znaki node'ów Text, rasteryzuj glify `font.rasterize(ch, font_size)` i `data as Rgba8u`-blend w obrysie `spana` (pomaluj ręcznie przez `pixels_mut` — tiny-skia służy tu tylko do fill; blend glifu zrób algorytmem alpha-over na pikselach). Wszystkie rysowania clip do pixmap (pętla po `max(0)`..`min(dim)`).

- [ ] **Step 4: Testy przechodzą**

Run: `cargo test -p prelix-render`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add prelix-render/src && git commit -m "feat(render): rasterize rects, borders and text"
```

### Task 6: API `render()` + host CLI → PNG

**Files:**
- Modify: `prelix-render/src/lib.rs` (dodaj `pub fn render`)
- Modify: `host/src/main.rs`, `host/Cargo.toml`
- Test: `prelix-render/src/lib.rs` (testy modułu)

**Interfaces:**
- Consumes: wszystko z Tasks 1–5.
- Produces:
```rust
pub struct RenderError;   // komunikat w Display
pub fn render(html: &str, css: &str, viewport: (u32, u32),
    font_bytes: &[u8]) -> Result<tiny_skia::Pixmap, RenderError>
```

- [ ] **Step 1: Napisz test (failing)**

Test (w `prelix-render/src/lib.rs`, wymaga feature `std` tylko dla testów — testy hostowe są ok, bo `cargo test` działa na hoście):
- `renders_empty_page_to_pixmap`: `render("", "", (100, 100), assets_font_bytes())` → Ok, pixmap 100×100, pixel (0,0) alpha 0.
- `render_error_on_bad_font`: `render("", "", (10, 10), b"not a font")` → Err.
- `end_to_end_rect`: `render("<div></div>", "* { display: flex; } div { background-color: red; width: 100px; height: 100px; }", (200, 200), assets_font_bytes())` → pixel (50,50) czerwony.

- [ ] **Step 2: Uruchom, sprawdź że nie przechodzi**

Run: `cargo test -p prelix-render`
Expected: FAIL.

- [ ] **Step 3: Zaimplementuj render() i host CLI**

`render`: `Font::from_bytes` → mapuj błąd na `RenderError`; potem `parse_html` → `parse_stylesheet` → `style_tree` → `compute_layout` → `paint`. `host/src/main.rs`: CLI `host <input.html> <styles.css> <output.png> [width] [height]` — czyta pliki, woła `render`, zapisuje PNG przez `image::RgbaImage` z `pixmap.pixels()` (konwersja ręcznie RGBA8). Panic z czytelnym komunikatem na `Err`.

- [ ] **Step 4: Testy przechodzą + smoke test PNG**

Run: `cargo test -p prelix-render && cargo run -p host -- examples/hello.html examples/hello.css /tmp/opencode/hello.png 320 240`
Expected: PASS; PNG istnieje. Utwórz `examples/hello.html` (nagłówek div, span z tekstem) i `examples/hello.css` do smoke testu.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(render): render() API and host PNG CLI"
```

### Task 7: Kernel — framebuffer + blit + QEMU

**Files:**
- Modify: `kernel/src/main.rs`, `kernel/Cargo.toml`
- Create: `kernel/src/framebuffer.rs`, `kernel/src/heap.rs`, `kernel/assets/page.html`, `kernel/assets/page.css`, `kernel/assets/font.ttf` (kopia z `prelix-render/assets/`)
- Modify: `os/src/main.rs`, `os/build.rs`, `os/Cargo.toml`
- Create: `run.sh`

**Interfaces:**
- Consumes: `prelix_render::render` (Task 6), `bootloader_api::BootInfo.framebuffer`.

- [ ] **Step 1: Sterta i entry point**

`kernel/src/heap.rs`: `#[global_allocator]` na `linked_list_allocator::LockedHeap` (init przez `unsafe { HEAP.lock().init(ptr, size) }` w `kernel_main`, obszar np. 1 MiB z `boot_info.physical_memory_offset` — wystarczy statyczny `static mut HEAP_MEM: [u8; 1024*1024]` za `Box::leak`-like raw ptr). `main.rs`: `bootloader_api::entry_point!(kernel_main)`, `kernel_main(boot_info: &'static mut bootloader_api::BootInfo) -> !`.

- [ ] **Step 2: Blit rendera na framebuffer**

`kernel/src/framebuffer.rs`: z `boot_info.framebuffer` czytaj `info()` → width, height, stride, `bytes_per_pixel`; format BGAAx/BGRx — mapuj kanały wg `PixelFormat` z bootloader_api (nie zakładaj RGBA). Renderuj przez `prelix_render::render(include_str!("../assets/page.html"), include_str!("../assets/page.css"), (fb_width, fb_height), include_bytes!("../assets/font.ttf"))`, potem kopiuj piksele wiersz po wierszu z uwzględnieniem `stride` i przestawieniem kanałów. Jeśli `render` zwróci `Err` lub framebuffer nie istnieje — panic z komunikatem (widać w QEMU przez `-d int` / serial; dodaj prosty `println!`-proxy na port 0x3F8 tylko do tego komunikatu, bez pełnego sterownika serial).

- [ ] **Step 3: Obraz dysku i QEMU**

`os/build.rs`: artefakt kernela z `std::env::var_os("CARGO_BIN_FILE_KERNEL_kernel")`, obraz przez `bootloader::BiosBoot::new(dest, kernel_path)?.finish()?` (wg `docs/create-disk-image.md` crate'a bootloader 0.11). `os/src/main.rs`: `std::process::Command::new("qemu-system-x86_64")` z `-drive format=raw,file=<obraz>` (ścieżka z `CARGO_MANIFEST_DIR/target`). `run.sh`: `cargo run -p os --release`.

- [ ] **Step 4: Weryfikacja w QEMU**

Run: `timeout 20 ./run.sh` w jednym terminalu + screendump: QEMU monitor (`-monitor stdio` dodane w os/main.rs), komenda `screendump /tmp/opencode/qemu-screen.ppm` po ~5 s.
Expected: w `qemu-screen.ppm` widoczny wyrenderowany HTML (ten sam układ co `/tmp/opencode/hello.png` z Task 6). Zweryfikuj programowo porównaniem szerokości/wysokości i obecności pikseli tła nagłówka (mały skrypt lub `python3` z PIL/`pnmtopng`; dopuszczalny human check opisany słownie w PR-owym opisie commita).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(kernel): boot, heap, framebuffer blit of rendered HTML in QEMU"
```
