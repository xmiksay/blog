# miksanik.net — Design System

A high-tech, terminal-flavored design system for **miksanik.net**, the personal site & knowledge base of **Martin Mikšaník** — software architect, systems engineer, crypto / smart-contract dev, and hobbyist (electric guitar, piano, analog photography, chess).

The current live site is intentionally minimal. This system gives it the character it deserves: **a sense that the person behind it is an IT person**. Circuit traces, monospace, deep blacks, and red/blue/violet/indigo signal accents. Less neon, more oscilloscope. Less "cyberpunk wallpaper", more "hand-soldered PCB under a desk lamp".

---

## Source material

- **Live site:** https://miksanik.net (Czech, very minimal — `Vítejte` / `About` / `Partie` / `Blog`)
- **About page:** https://miksanik.net/about
- **GitHub:** https://github.com/xmiksay
- No codebase, Figma, or screenshot pack was attached. Visuals here are derived from the brief + the live pages.

## What the site is

A Czech personal blog & znalostní báze (knowledge base). Pages are authored in **Markdown** with custom directives:

| Directive | Purpose |
|---|---|
| `::page{path=...}` | **Transclude** another page inline. Should render with a subtle frame and be **collapsible**. |
| `::img` | Embed an image |
| `::gallery` | Image gallery |
| `::pgn` | Render a chess game (PGN) via chessboard engine |
| `::fen` | Render a chess position (FEN) |

The system below treats those directives as first-class UI components — each gets a defined visual treatment.

---

## Index

| File | What's there |
|---|---|
| `README.md` | This file. Context, content + visual + iconography fundamentals, manifest. |
| `colors_and_type.css` | All design tokens — colors, type ramp, spacing, radii, shadows, semantic vars. |
| `fonts/` | Web fonts (Google Fonts links — see CAVEATS). |
| `assets/` | Logo mark, circuit-trace SVG, placeholder imagery. |
| `preview/` | Card files for the Design System tab. |
| `ui_kits/site/` | UI kit: page renderer + directive components + sample interactive screen. |
| `SKILL.md` | Cross-compatible skill manifest. |

---

## CONTENT FUNDAMENTALS

**Language.** Czech first, English second. Most user-facing copy is Czech; technical labels (filenames, code, command palette) are English. Don't translate code, package names, or chess notation.

**Voice.** First person, plain, unhyped. Martin writes for himself first and lets others read along. Closer to a lab notebook than a marketing site. *"Osobní blog a znalostní báze. Najdete tu poznámky k technologiím, které mě zajímají, a občas něco navíc."* sets the tone — direct, practical, slightly understated.

**Casing.** Sentence case for headings (`Vítejte`, `O mně`, not `O Mně`). Brand names keep their original casing (`PostgreSQL`, `Rust`, `Aiken`). All-caps is reserved for terminal-style labels and section dividers (`// SECTION`, `OUTPUT`).

**Person.** "Já" / "mě" — first person singular. Don't address the reader as "you" / "vy" except in direct calls to action (e.g. `Najdete tu...`).

**Punctuation & dashes.** Em-dash for asides (`— kdo jsem, co dělám, kontakt`). No Oxford comma in Czech. Periods at end of full sentences; bare phrases in tables/lists left bare.

**Numbers & units.** Years as ranges with en-dash (`2009–2013`). Months abbreviated English in tables (`Apr 2021 – Nov 2025`). No "AI-generated" filler stats.

**No emoji.** The site doesn't use them and shouldn't. Use unicode glyphs sparingly (`§ ¶ ↗ → ⌘ ▸ ▾ ◆ ▪`), never as decoration.

**Tone examples (lifted from the live site):**

> *"Vítejte. Osobní blog a znalostní báze. Najdete tu poznámky k technologiím, které mě zajímají, a občas něco navíc."*

> *"Software architecture, algorithmization, encryption, decentralization, microservices — skiing, wakeboarding, snowboarding, motorcycles, electric guitar, piano, analog photography"*

Note the second one: a single em-dash separates the technical from the personal. That structural move — **"work things — life things"** — is a load-bearing rhetorical pattern. Use it.

**Don't.** No "Welcome to my digital home." No "passionate about clean code." No "🚀 ✨ 💡". No marketing voice. No "we" — there's no we.

---

## VISUAL FOUNDATIONS

### The vibe in one sentence
**A terminal pulled up next to a soldering iron, with a chess board on the side.** Dark background, monospace type, hairline lines that look like circuit traces, occasional glow on a single accent — not the whole UI.

### Color
- **Base:** near-black `#07090C` and `#0B0F14` — not pure black; slight cool cast, like a CRT in a dark room.
- **Foreground:** off-white `#E6EDF3` (terminal-text grey, never `#fff`).
- **Trace lines:** muted indigo / steel `#1E2A3A` for borders, `#2A3A52` for hover.
- **Accents (use ONE per surface, not all five):**
  - Signal red `#FF3B4E` — errors, recording, "live", chess black-to-move.
  - Capacitor blue `#3B82F6` — links, focus, info.
  - Plasma violet `#8B5CF6` — highlights, "chess" brand color.
  - Indigo `#6366F1` — secondary highlight.
  - Phosphor green `#22D3A0` — success, "compile ok", chess white-to-move. Used SPARINGLY; we are not a 1980s green-screen.
- **Semantics:** muted by default. A status pill is `bg: rgba(accent, 0.12)` + `border: rgba(accent, 0.35)` + `fg: full accent`. **No saturated fills on large areas.**

### Type
- **Display:** `JetBrains Mono` — for the wordmark, hero titles, and big section headers. Yes, mono for display. It's the look.
- **Body:** `Inter` for long-form Czech prose (better diacritics rendering than mono at body sizes). 16px base, 1.65 line-height.
- **Code / mono UI / chess notation:** `JetBrains Mono`, 14px.
- **Headings:** Display weights 600/700, generous tracking (`letter-spacing: -0.01em` on h1/h2; `+0.08em uppercase` on section labels).
- **No serif.** Period.

### Spacing
4px base scale: `4 / 8 / 12 / 16 / 24 / 32 / 48 / 64 / 96`. Page gutter on desktop: 32px. Reading column max-width: **68ch** (not pixels — tied to the actual reading rhythm).

### Borders & corners
- **Radii are SMALL.** `2px` for chips, `4px` for cards, `8px` for big surfaces (modals). Nothing pill-shaped except the live "REC" dot.
- Borders are 1px hairlines in `#1E2A3A`. On hover they brighten to `#2A3A52`. On focus, accent at 60%.
- **Circuit-trace decoration:** thin SVG paths in `#1E2A3A` running along card edges, with a single solder-pad dot at intersections. Optional, used to frame hero sections — never decorating buttons.

### Shadows & elevation
- Almost none. The aesthetic is flat dark + bright hairline borders. When elevation is needed: `0 0 0 1px #1E2A3A, 0 8px 24px rgba(0,0,0,0.5)`.
- For "active signal": `0 0 0 1px <accent>40, 0 0 24px <accent>30` — a soft phosphor halo. Use it on `:focus-visible` and on the currently-playing chess move. Never as a default state.

### Backgrounds
- Solid near-black (`--bg-0`), with optional **PCB grid**: a 24×24px SVG of dotted vias + faint horizontal/vertical traces at 4% opacity. Lives behind hero / About header / 404. Not on every page.
- For full-bleed content (analog photo blog posts), allow a single image with a 1px hairline + the photo's natural tonality. No filters, no gradient overlays — film grain stays on the photo.

### Animation
- Easing: `cubic-bezier(0.2, 0.8, 0.2, 1)` (a measured "settle"). 150–220ms for state changes, 320ms for layout shifts.
- **No bounce.** No spring overshoot. This is engineering tooling, not a fitness app.
- One signature motion: a 1px **scanline sweep** on big section transitions (`transform: translateY` of a `1px` line down 100% over 600ms, then fade). Used on page mount of /partie and /about. Not on every page.
- Hover: 120ms color/border tween, no scale, no shadow growth.
- Press: 80ms `opacity: 0.85`, no shrink.

### Hover / press / focus / active
| State | Treatment |
|---|---|
| Hover (link/button) | Border or text shifts toward accent at full opacity; bg fill lifts from 0% → 8%. |
| Press | `opacity: 0.85`, no transform. |
| Focus-visible | 1px accent ring + soft halo (`0 0 0 1px accent, 0 0 12px accent33`). |
| Disabled | `opacity: 0.4`, no other change. |

### Imagery vibe
- **Cool, low-saturation.** The site is dark; images should not blow out against it. For analog-photo posts, accept the original tonality but suggest a 1px hairline frame.
- For decorative imagery, **circuit-board macro shots** (PCBs, traces, solder, oscilloscopes) > generic stock. Never AI-rendered "tech abstract" gradients.
- Galleries: 2/3/4-up grid, 8px gap, no captions floating over images — captions sit below in mono caption style.

### Cards
A card = `1px hairline border` + `2px radius` + `bg-1 fill` + `16-24px padding`. No drop shadow by default. The corners may have **tiny corner brackets** (`└ ┐ ┘ ┌`) drawn as 8px L-shaped SVG marks at each corner — a quiet nod to terminal frames. Optional, used on the most "important" card on a page, not on every one.

### Transparency / blur
- Glass blur is forbidden on body content. The one place it appears: a sticky top nav uses `backdrop-filter: blur(8px)` + `bg-0 / 80%` so the trace pattern shows through faintly.

### Layout rules
- Single fixed header (56px). No fixed footer. No sidebar by default; admin/editor view introduces a left rail.
- Reading view is centered, 68ch. Wide views (galleries, chess board, code listings) break out to 100% column.
- Page fades in at 120ms; the scanline only on /about and /partie landing.

### The transclude treatment (load-bearing)
A `::page{path=...}` block renders inside its host page as:
- A **left vertical bar** (2px wide, `--accent-violet`).
- A header strip showing the transcluded path in mono (`▸ /notes/foo`) + a collapse toggle (`▾ ▸`).
- The body indented 12px inside the bar.
- Collapsed state shows only the header strip + summary line.
This is the design system's most distinct component — get it right.

---

## ICONOGRAPHY

**Approach.** Hairline, monoline, 1.5px stroke, 24px artboard. The site doesn't currently ship its own icon set — for this system we use **Lucide** (`lucide.dev`) via CDN, because its 1.5px monoline matches the aesthetic exactly. Documented in `assets/ICONOGRAPHY.md`.

**Substitutions flagged:** Lucide is a substitution — the live site has no formal icon set. If Martin wants a custom set later, that's a follow-up.

**Unicode glyphs.** Used as inline markers, not as full icons:
- `→ ↗` for outbound links
- `▸ ▾` for collapsible regions (transclude headers!)
- `§ ¶` for anchors and inline references
- `◆` for list markers in spec/manifest pages
- `·` for separators in metadata strips

**No emoji.** Anywhere. Ever.

**SVGs ship inline** for: the wordmark, the circuit-trace decoration, the chess piece set (when used outside the chessboard engine).

**The chess board** is rendered by the existing chessboard engine (presumably chessground or chessboard.js — flagged as TBD; Martin's preference goes here). The system reserves a violet (`--accent-violet`) for chess accents.

---

## CAVEATS / FLAGGED SUBSTITUTIONS

- **Fonts:** No font files were provided. Using Google Fonts CDN for `JetBrains Mono` + `Inter`. Swap in self-hosted woff2 if Martin wants offline support.
- **Icons:** Lucide is a substitution; no official icon set exists.
- **Chess engine:** Spec says "chessboard engine" — exact library unknown. UI kit shows a styled placeholder + treats moves/notation as the actual content.
- **No codebase / Figma:** Visual decisions extrapolated from the brief + the live site. Open to course-correction.

---

## NEXT — what to give me to make this perfect

1. The repo URL or a code paste of how `::page` `::img` `::gallery` `::pgn` `::fen` are rendered today.
2. A screenshot of the editor (you mentioned a markdown editor — built-in or external?).
3. Which chessboard library (`chessground`, `chessboard.js`, custom?).
4. Self-hosted font files if you want to drop Google Fonts.
5. Any existing color/spacing tokens in the codebase.
