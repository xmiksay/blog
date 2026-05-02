# Bíločerný Ořechov — Design System

A design system for **Šachový klub Bíločerný Ořechov, z.s.** — a chess club based in the village of Ořechov near Brno, Czech Republic. Founded **15 April 2025**, the club organizes tournaments, trains youth, and represents Ořechov in regional competitions.

> Note on the name: the user-supplied brief used "Černo-bílý Ořechov" (black-and-white). The live site and the club's about page both say **"Bíločerný Ořechov"** (white-and-black). This system uses the live-site name. Please confirm which is canonical.

The site is a simple CMS where editors add content pages organized under a 2-level menu (groups + items). The admin UI exists; this design system targets the **public-facing pages** (top header + left menu + content area + footer).

---

## Sources

- **Live site (only source):** https://orechov.sourcelab.cz/
  - Listing page: `/clanky`
  - Article example: `/clanky/1` (Club "About" page with gallery)
  - Admin login: `/admin/login`
- **No codebase, Figma, or logo files were provided.** The current live site uses no graphics — text-only with a chess-king unicode glyph (♚) as the brand mark. Everything visual in this system is newly proposed.

---

## Index

| File | What's in it |
|------|--------------|
| `README.md` | This file — brand context, content rules, visual foundations, iconography |
| `colors_and_type.css` | Color + typography CSS variables (semantic + raw) |
| `fonts/` | Web fonts (loaded via Google Fonts CDN — see below) |
| `assets/` | Logos, chess piece SVGs, decorative assets |
| `preview/` | Cards rendered in the Design System tab |
| `ui_kits/website/` | High-fidelity React/JSX components for the public site |
| `SKILL.md` | Agent Skill manifest — drop this folder into Claude Code |

### Folder manifest

```
README.md                       — this file
SKILL.md                        — Agent Skill front-matter for Claude Code
colors_and_type.css             — design tokens + base element styles
assets/
  logo-horizontal.svg           — wordmark + knight glyph (header use)
  logo-mark.svg                 — square mark with frame (favicon, posters)
  pieces/
    knight.svg                  — hero piece, brand mark glyph
    king.svg  queen.svg  rook.svg  bishop.svg  pawn.svg
preview/                        — small Design-System-tab cards
  brand-logo.html · brand-pieces.html · brand-divider.html
  colors-core.html · colors-ink-scale.html · colors-semantic.html
  type-display.html · type-headings.html · type-body.html · type-mono.html
  spacing-scale.html · spacing-radii-shadows.html
  components-buttons.html · components-fields.html · components-menu.html
  components-card.html · components-header.html · components-markdown.html
ui_kits/
  website/
    README.md
    styles.css                  — extends colors_and_type.css, full .bn-rich for markdown
    index.html                  — interactive prototype (click sidebar / top nav)
    Header.jsx · Sidebar.jsx · Footer.jsx
    MarkdownArticle.jsx         — renders MD via marked + .bn-rich
    ArticleList.jsx · Tournaments.jsx · Contact.jsx
    app.jsx                     — composes the screens
```

### How the markdown rendering works

Live site articles are markdown. The `.bn-rich` class in `ui_kits/website/styles.css` styles **every** standard MD element so any markdown HTML output (from `marked`, `markdown-it`, `remark`, server-side Czech CMS, etc.) renders on-brand. Wrap output:

```html
<div class="bn-rich">
  {{ markdown_html }}
</div>
```

Covered elements: `h1`–`h6`, `p`, `ul`/`ol` (custom diamond + mono numerals), nested lists, GFM task lists, `blockquote` + `cite`, inline `code`, `pre`/`code` block (burgundy left rail), `table`/`thead`/`tbody`, `hr` (chessboard texture, not a flat line), `img` (sepia treatment), `a`, `strong`, `em`, `del`, `dl`/`dt`/`dd`, footnotes, `sup`.

---

## Brand at a glance

- **Identity:** small village chess club, friendly but rooted in chess tradition
- **Audiences:** adult members, juniors & families, tournament visitors, locals
- **Language:** Czech only
- **Color story:** strict black & white + one warm accent (burgundy/wine — see Visual Foundations)
- **Type:** serif headlines (editorial, classic-chess-column feel) + clean sans body
- **Motif:** the **knight** is the hero piece; chessboard pattern used as a divider/accent, not as wallpaper
- **Tone:** sober, welcoming, slightly traditional — not corporate, not overly playful

---

## CONTENT FUNDAMENTALS

The live site's copy is sparse but consistent. Rules drawn from `/clanky/1`:

### Voice & tone
- **Third-person, formal but warm.** "Klub vznikl s cílem rozvíjet šachovou komunitu…" — never "We" or "You". Sentences describe the club from outside.
- **Sober, factual, slightly civic.** Like a small-town spolek annual report. No marketing language ("revoluční", "úžasný"), no exclamation marks.
- **Czech only.** Use diacritics correctly (š, č, ř, é, í, ý, á). Never anglicisms where a Czech word exists.

### Casing
- **Headings:** Sentence case ("O nás", "Soutěže a aktivity"), not Title Case.
- **The club's name** is rendered with no special casing rules — `Bíločerný Ořechov` in body text. The legal form `z.s.` is lowercase.
- **Section dividers:** `---` horizontal rules between major sections in long articles.

### Pronouns
- Avoid first and second person where possible. Prefer "klub", "spolek", "členové", "hráči".
- When direct address is unavoidable (e.g. CTA on a tournament page), use polite plural: "Přihlaste se", not "Přihlaš se".

### Punctuation
- Sentences end with a full stop. Lists use bullets without trailing punctuation.
- Czech quotation marks „takto" preferred over `"…"` for body copy.
- Em-dash `—` for parenthetical asides.

### Specific examples (lift from the live site)

> **Bíločerný Ořechov, z.s.** je šachový klub působící v obci Ořechov nedaleko Brna.

> Hlavním účelem spolku je organizace a rozvoj šachové hry, vytváření podmínek pro trénink a soutěže, podpora mládeže a začátečníků…

> Klub se zároveň zaměřuje na budování komunity a podporu sportovních i etických hodnot.

### What to avoid
- ❌ Emoji. The brand is sober.
- ❌ "Vítejte na našich stránkách!" — this kind of empty welcome.
- ❌ Marketing superlatives.
- ❌ Slang or overly casual phrasing.
- ❌ Mixing English ("event", "match") where Czech words exist ("turnaj", "zápas").

### Standard nav labels (Czech)
- `Články` — Articles / news
- `O nás` — About
- `Turnaje` — Tournaments
- `Členové` — Members
- `Tréninky` — Training
- `Galerie` — Gallery
- `Kontakt` — Contact
- `Přihlásit` — Log in (admin)

---

## VISUAL FOUNDATIONS

The live site has none — it's a Bootstrap-default page with a unicode chess king. Everything below is **newly proposed**, grounded in the brief: black & white + warm accent, moderate chess motifs, knight as hero piece.

### Colors

A monochrome system with **one warm accent: deep burgundy** (`#7A1F2B`). Burgundy reads as serious, traditional, slightly civic — fits "village chess club" better than bright amber would. Used sparingly: links, CTAs, active menu states, the brand mark's accent stroke.

- **Ink (foreground):** near-black `#111111`, never pure `#000` — softer on eye on warm backgrounds
- **Paper (background):** off-white `#F7F4EC` — slight warm cream, evokes old chess-column newsprint
- **White:** `#FFFFFF` for cards/surfaces sitting on paper
- **Accent (burgundy):** `#7A1F2B` for links, primary actions, active states
- **Accent dark:** `#5A1620` for hover/pressed
- **Neutrals:** `#E8E2D4` (rule), `#C9C2B2` (muted text), `#5C564B` (secondary text)

See `colors_and_type.css` for the full palette + semantic tokens.

### Typography

- **Display / headings:** **Source Serif 4** (Google Fonts). A modern serif with character but not flashy — the "chess column" feel. Weights 400, 600, 700.
- **Body / UI:** **Inter Tight** (Google Fonts). Clean sans, slightly condensed, pairs well with Source Serif. Weights 400, 500, 600.
- **Mono / chess notation:** **JetBrains Mono** (Google Fonts). For move notation (`1. e4 e5 2. Nf3`).

> **Substitution flag:** No font files were provided. We use Google Fonts CDN. If the club has chosen typefaces, replace `colors_and_type.css` font URLs and add files to `fonts/`.

#### Type scale (1.250 major-third)
- `--font-size-xs`: 12px
- `--font-size-sm`: 14px
- `--font-size-base`: 16px
- `--font-size-md`: 18px (long-form body)
- `--font-size-lg`: 22px
- `--font-size-xl`: 28px
- `--font-size-2xl`: 36px
- `--font-size-3xl`: 48px (page titles)

### Spacing

8-px base grid. Tokens: `--s-1` (4), `--s-2` (8), `--s-3` (12), `--s-4` (16), `--s-5` (24), `--s-6` (32), `--s-7` (48), `--s-8` (64).

### Backgrounds & textures

- **Primary background:** flat `--paper` (#F7F4EC).
- **Section accent:** very subtle 8×8 chessboard pattern at 4% opacity, used as a divider band between major sections. Never as full-bleed wallpaper.
- **No gradients.** No noise textures beyond the chessboard pattern. No full-bleed photos behind text.
- **Imagery:** when galleries are shown, photos are presented in B&W or warm-sepia treatment to harmonize with the palette (CSS `filter: grayscale(0.4) sepia(0.15);`).

### Borders & rules

- 1px solid `--rule` (#E8E2D4) for dividers.
- 2px solid `--ink` for emphasis (e.g. under page title).
- No borders on cards by default — separation comes from background contrast (`--white` card on `--paper` page).

### Shadows

Single soft shadow token, used sparingly:
- `--shadow-sm`: `0 1px 2px rgba(17,17,17,0.06)`
- `--shadow-md`: `0 4px 12px rgba(17,17,17,0.08)`

No inset shadows. No glow effects.

### Corner radii

Modest, slightly traditional:
- `--radius-sm`: 2px (inputs)
- `--radius-md`: 4px (buttons, cards)
- `--radius-lg`: 8px (modals)
- Full circle for avatars only.

### Animation & motion

- **Easing:** `cubic-bezier(0.4, 0, 0.2, 1)` (ease-out, slightly fast).
- **Duration:** 150ms for hover/press, 220ms for menu reveal.
- **No bounces, no spring physics.** No long fades, no parallax.
- Page transitions: instant. The site is functional, not flashy.

### Hover & press states

- **Links:** color shifts from `--accent` → `--accent-dark`, plus 1px solid underline appears (chess-column feel).
- **Buttons (primary):** background `--accent` → `--accent-dark`, no scale change.
- **Buttons (secondary, ghost):** background fills from transparent → `--ink-5` (5% black).
- **Press:** translate Y by 1px on press, no scale.
- **Menu items:** background fills `--ink-5`; active item has 3px `--accent` left bar + bold weight.

### Layout rules

- **Top header:** fixed, 64px tall, white background, 1px bottom rule.
- **Left menu:** fixed, 260px wide, paper background, 1px right rule.
- **Content max-width:** 720px for long-form text (readability), 1080px for galleries/grids.
- **Page padding:** 32px top/sides on desktop, 16px on mobile.
- **Footer:** simple, single line, 1px top rule.

### Transparency & blur

- Used only for the mobile-menu backdrop (`rgba(17,17,17,0.4)` + `backdrop-filter: blur(4px)`).
- Never on cards, headers, or text.

### Image treatment

- **Galleries:** B&W or warm-sepia (`grayscale(0.4) sepia(0.15)`) by default — restored to color on hover for accessibility/interest.
- **Aspect:** 4:3 thumbnails in galleries, 16:9 for headers.

### Cards

- White background on paper page.
- 4px corner radius.
- No border, just `--shadow-sm`.
- 24px internal padding.
- Title in serif, body in sans.

### Visual hierarchy summary

1. The **knight** logo + wordmark are the only places the brand "shows off".
2. The **page title** uses serif at 48px with a 2px black underline rule — newspaper masthead energy.
3. Burgundy is rare — when you see it, it's interactive.
4. The chessboard pattern is a quiet divider, never a stage.

---

## ICONOGRAPHY

The live site uses **no icons** beyond the unicode chess king `♚` as a brand mark. This system proposes:

### Approach
- **Lucide** (https://lucide.dev) icon set, loaded via CDN, for all UI icons (menu chevrons, search, calendar, user, etc). Stroke-based, 1.5px stroke, modern but neutral — pairs well with the editorial type.
- **Custom chess piece SVGs** in `assets/pieces/` — minimal, single-color, B&W. The knight is the brand mark; other pieces (king, rook, bishop, pawn, queen) are available for use in headers, dividers, decorative spots.
- **Unicode chess glyphs** (♔♕♖♗♘♙ / ♚♛♜♝♞♟) acceptable for inline text use (e.g. inside an article body) — the live site already uses ♚.
- **No emoji.** Ever.
- **No raster (PNG) icons.** Everything SVG or icon-font.

### Substitution flag

The chess piece SVGs in `assets/pieces/` are derived from public-domain Wikimedia (Cburnett's standard chess set), restyled to single-color black. The Lucide icon set is unmodified open-source. **Flag this to the club** — if they want a custom illustrated piece set, this is a placeholder.

### Usage rules
- Icons sit at 16×16 (inline) or 20×20 (menu items, buttons).
- Always inherit color from `currentColor`.
- Never combine icon + emoji.
- The knight logo lives only in the top-left of the header. Don't decorate other areas with the full logo.
