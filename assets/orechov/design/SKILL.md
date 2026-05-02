---
name: bilocerny-orechov-design
description: Use this skill to generate well-branded interfaces and assets for Šachový klub Bíločerný Ořechov, either for production or throwaway prototypes/mocks/etc. Contains essential design guidelines, colors, type, fonts, assets, and UI kit components for prototyping. The site is markdown-driven, so the system covers all common MD primitives.
user-invocable: true
---

Read the README.md file within this skill, and explore the other available files.

Key files:
- `README.md` — brand context, content fundamentals, visual foundations, iconography
- `colors_and_type.css` — drop-in CSS variables (colors, type, spacing, radii, shadows, motion)
- `assets/` — knight logo (horizontal + mark), full chess piece SVG set, Cburnett-derived
- `ui_kits/website/` — JSX components (Header, Sidebar, MarkdownArticle, ArticleList, Tournaments, Contact, Footer) + styles.css with rich `.bn-rich` rules covering every standard markdown element (h1–h6, p, ul/ol, blockquote, code, pre, table, hr, img, a, strong/em, dl, footnotes, task lists)

If creating visual artifacts (slides, mocks, throwaway prototypes, etc), copy assets out of `assets/` and import `colors_and_type.css` + `ui_kits/website/styles.css` to inherit the system. For markdown-rendered article pages, wrap the rendered HTML in `<div class="bn-rich">` — that selector targets every MD primitive with branded styling.

If working on production code, copy `colors_and_type.css` and the relevant component(s) from `ui_kits/website/` and adapt to your framework. Component implementations are deliberately simple (mostly cosmetic); copy the structure, lift the CSS verbatim.

If the user invokes this skill without any other guidance, ask them what they want to build or design (a new content page? a tournament listing? a poster?), ask a few questions about content and audience, and act as an expert designer who outputs HTML artifacts _or_ production code, depending on the need. Always render Czech text correctly with proper diacritics.
