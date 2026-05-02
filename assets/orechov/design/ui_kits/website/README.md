# Bíločerný Ořechov — Web UI Kit

High-fidelity recreation of the public-facing site. The live source (https://orechov.sourcelab.cz) is plain Bootstrap; this kit applies the design system to the same information architecture.

## Components

- `Header.jsx` — Top bar (64px, white, knight glyph, top-level nav, login)
- `Sidebar.jsx` — 2-level left menu (groups + items, active state, expandable)
- `PageMasthead.jsx` — Article title + meta (newspaper masthead)
- `ArticleCard.jsx` — Listing item for /clanky
- `EventCard.jsx` — Tournament/training entry
- `Gallery.jsx` — Sepia-tinted image grid (hover to restore color)
- `Footer.jsx` — Single-line footer with sitemap link
- `RichText.jsx` — Renders article body (h2/h3/p/ul/hr) with system styles

## Screens (in `index.html`)

1. **Articles list** (`/clanky`) — default view
2. **Article detail** — long-form with gallery
3. **Tournaments** — event cards in a grid
4. **Contact** — simple two-column

Click the top nav or sidebar to switch screens. State is fake (no routing).
