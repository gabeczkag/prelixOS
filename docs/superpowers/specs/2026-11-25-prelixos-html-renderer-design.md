# prelixOS — renderer HTML/CSS w Ruście (design)

Data: 2026-11-25 · Status: do zatwierdzenia

## Intencja i dlaczego

prelixOS ma być systemem operacyjnym w Ruście, w którym interfejs aplikacji
opisuje się w HTML (frontend), a całą logikę i rysowanie wykonuje silnik
w Ruście (backend). HTML jest deklaratywnym opisem UI — nie budujemy
przeglądarki internetowej: bez sieci, JS i pełnego CSS.

## Cele

- Kernel w Ruście bootujący przez `bootloader` 0.11 z dostępem do framebuffera.
- Renderer HTML/CSS jako osobny crate (`prelix-render`), rdzeń `no_std + alloc`.
- Ten sam renderer działa na hoście (Linux, tryb `std`, render do PNG) i
  w kernelu — szybka iteracja po wyglądzie bez reboota QEMU.
- Na start obsłużony podzbiór: `div`, `span`, tekst, `img`; CSS: kolory,
  rozmiary, `padding`/`margin`, flexbox, obramowania.

## Non-goals / anti-goals

- Nie budujemy przeglądarki: zero sieci, zero JS, zero pełnych selektorów CSS.
- Nie używamy `html5ever` w kernelu (wymaga `std`, vendorowanie 3+ crate'ów).
- Bez GPU i shapingu tekstu — prosty rasteryzer fontów (fontdue), font osadzony
  w obrazie systemu.

## Ograniczenia

- Bare-metal: brak `std`, własna sterta (`linked_list_allocator`).
- Biblioteki muszą mieć tryb `no_std`: taffy (feature `alloc`), tiny-skia
  (`no-std-float`), fontdue.
- Kryterium sukcesu pierwszego etapu: strona HTML wyrenderowana poprawnie
  do PNG na hoście ORAZ do framebuffera w QEMU tym samym kodem rdzenia.

## Podejścia rozważone

1. **Bare-metal od razu** — prawdziwe, ale tygodnie infrastruktury przed
   pierwszym efektem wizualnym. Odrzucone: konflikt z „szybko”.
2. **Renderer na Linuksie najpierw** — szybkie efekty, ale kod ze `std`
   trzeba by później przepisać. Odrzucone: dwukrotna praca.
3. **Hybryda (wybrana)** — rdzeń renderera `no_std + alloc`, warstwa `std`
   tylko na hoście (PNG, testy). Ten sam kod w kernelu i na hoście.

## Decyzje architektoniczne (moje, do sprawdzenia)

- Pipeline renderera: parser HTML → DOM → parser podzbioru CSS → stylowanie →
  layout (taffy) → rasteryzacja (tiny-skia + fontdue) → bufor pikseli.
- Własny minimalny parser HTML (stos/tagi/atrybuty/tekst), ~kilkaset linii.
- Kernel: `bootloader` 0.11, sterta, framebuffer z bootloadera, blit bufora
  renderera na framebuffer.
- Host: crate z feature `std` zapisuje wynik do PNG; testy porównawcze.
- Interaktywność (zdarzenia klik/klawiatura, most HTML↔Rust) — etap 2,
  po pierwszym renderingui.

## Co zostawiam wykonawcy

- Struktura crate'ów (workspace: `kernel`, `prelix-render`, `host-tools`).
- Szczegóły parsera CSS (składnia deklaracji, jednostki px/%).
- Wybór fontu i jego osadzenie.
- Plan etapów i kolejność implementacji.

## Etapy (szkic)

1. Workspace + szkielet kernela bootujący się w QEMU.
2. Sterta + framebuffer, prosty blit (kwadrat na ekranie).
3. `prelix-render`: parser HTML → DOM, render tekstu na PNG na hoście.
4. CSS podzbiór + taffy layout + tło/border.
5. Blit tego samego renderowania na framebuffer w QEMU.
6. (Etap 2) zdarzenia i interaktywność.
