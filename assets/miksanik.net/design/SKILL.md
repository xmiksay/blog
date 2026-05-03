# miksanik.net — Design system

A light, terminal-flavored design system for **miksanik.net**, the personal blog & znalostní báze of **Martin Mikšaník** (software architect, smart-contracts dev, chess hobbyist). Use it for any HTML mockup, prototype, or screen that should feel like part of his site.

## What it looks like
- **Light content area, dark sidebar** with a 1px violet trace on the inner edge.
- **Mono-as-display** — JetBrains Mono for headers, Inter for Czech body prose.
- **Hairline borders, small radii (2/4/8px), almost no shadows.** Engineering-tooling, not marketing.
- **Five accent signals used sparingly:** red (live/error), blue (link/info), **violet (brand / chess)**, indigo, green.
- **Czech-first copy.** Sentence case, em-dash for asides, no emoji.

## Always start by reading
- `README.md` — full content, visual, iconography fundamentals + caveats.
- `colors_and_type.css` — every token. Import this in every kit/page.

## Key components (in `ui_kits/site/`)
- `Sidebar.jsx` — dark nav with collapsible tree, violet active state.
- `Transclude.jsx` — the `::page{path=...}` directive: violet left bar + collapsible header. **Load-bearing**.
- `ChessViewer.jsx` — `::pgn` / `::fen` board with violet halo and slate/navy squares.
- `Gallery.jsx` — `::img` / `::gallery` with mono captions.
- `Editor.jsx` — markdown editor with Czech directive shortcuts (Vložit ::page, ::img, ::gallery, ::pgn, ::fen).

## Directives (the site's content vocabulary)
| Directive | Component | Note |
|---|---|---|
| `::page{path=...}` | Transclude | Render inline, collapsible, violet bar |
| `::img{src= alt= caption=}` | Article image | 1px hairline frame, mono caption |
| `::gallery` | Grid | 2/3/4-up auto-fill, 8px gap |
| `::pgn{...}` | ChessViewer | Notation in violet, navy/slate squares |
| `::fen{...}` | Static board | No controls |

## When making a new page
1. Wrap in `<div class="layout">` with `<aside class="sidebar">` + `<main>`.
2. Use Czech sentence-case headings; `JetBrains Mono` weight 700, tight tracking.
3. Reading column max-width **68ch**.
4. Use **one** accent per surface — violet is brand default; reach for blue/red/green only with intent.
5. No emoji. Use `▸ ▾ → ↗ §` glyphs sparingly.

## Caveats
- Fonts loaded via Google Fonts CDN. Self-host if needed.
- Icons: substitution flagged — Lucide via CDN; no official set yet.
- Chessboard: real site uses chessboard.js + chess.js; kit uses a styled placeholder with the same DOM shape (`.pgn-viewer[data-pgn]`).
