# UI Kit — miksanik.net

Drop-in dark-theme recreation of the existing Rust/Axum SSR site (`xmiksay/site`).

**DOM-compatible.** Same class names as `assets/common/css/style.css` — `.layout`, `.sidebar`, `.content`, `.article-list`, `.transclude`, `.pgn-viewer`, `.gallery-grid`, `.md-toolbar`, etc. The styles in `colors_and_type.css` plus the kit-local CSS are intended to fully replace `style.css` while keeping the templates untouched.

## Files
- `index.html` — interactive demo: list view → article detail (with all directives) → editor.
- `Sidebar.jsx` — navigation rail (matches `templates/base.html` macro).
- `Article.jsx` — `.article-detail` with `.article-body` markdown surface.
- `Transclude.jsx` — `::page{path=…}` collapsible block.
- `ChessViewer.jsx` — `::pgn` + `::fen` shell (real engine drops in unchanged).
- `Gallery.jsx` — `::gallery` grid + `::img` figure.
- `Editor.jsx` — admin markdown editor with toolbar + preview toggle.
- `app.jsx` — wires it together with fake routing.

## Multi-tenant note
The site supports multiple tenants (miksanik.net, orechov, …). This kit covers **miksanik.net** only — the orechov tenant has its own design system at `assets/orechov/design/` in the codebase.
