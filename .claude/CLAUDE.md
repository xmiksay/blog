# CLAUDE.md — Personal site

## Overview

Server-rendered personal site. Rust/Axum backend with PostgreSQL, MiniJinja templates, and an admin CMS for managing pages, images, galleries, tags, and navigation menus. Auth via Bearer tokens; service tokens for MCP integration.

## Tech Stack

- **Backend:** Rust (edition 2024), Axum 0.8, Tokio
- **Database:** PostgreSQL via SeaORM 1.x
- **Templates:** MiniJinja with `templates/` directory
- **Markdown:** pulldown-cmark for page rendering
- **Auth:** Argon2 password hashing, Bearer token sessions, service tokens for MCP
- **Frontend:** Static HTML/CSS/JS served from `static/`, Tailwind CSS
- **Logging:** tracing + tracing-subscriber with env filter

## Project Structure

```
src/
  bin/
    site_server.rs      # Main web server (port 3000)
    site_migration.rs   # Migration CLI (up/down/fresh/status)
    site_cli.rs         # Admin CLI (create-user)
  entity/               # SeaORM entity models
    user.rs, token.rs, tag.rs, menu.rs, page.rs,
    page_revision.rs, image.rs, gallery.rs, gallery_image.rs
  migration/            # SeaORM migrations
  routes/
    public/             # Public-facing routes (pages, images, galleries)
    admin/              # Admin CMS routes (auth-protected)
    mod.rs              # Shared view types, menu builder
  auth.rs               # Password hashing, token middleware
  markdown.rs           # Markdown rendering with custom extensions
  state.rs              # AppState (db + template env)
templates/              # MiniJinja HTML templates
static/                 # CSS, JS, images
```

## Build & Run

```bash
# Build
cargo build
cargo build --release

# Run (requires DATABASE_URL in .env or environment)
cargo run --bin site_server

# Migrations
cargo run --bin site_migration          # apply all
cargo run --bin site_migration -- down   # rollback last
cargo run --bin site_migration -- fresh  # reset & reapply
cargo run --bin site_migration -- status

# Create user
cargo run --bin site_cli -- create-user <username> <password>
```

## Environment

- `DATABASE_URL` — PostgreSQL connection string (default: `postgres://blog:blog@localhost:5432/blog`)
- `RUST_LOG` — optional, defaults to `site=debug,tower_http=debug,info`

## Data Model

```
users
  id (PK), username (unique), password_hash

tokens
  id (PK), nonce (unique), user_id (FK), expires_at
  -- Used for both session tokens (login) and service tokens (MCP)

tags
  id (PK), name (unique), description

menus
  id (PK), path (unique), markdown

pages
  id (PK), path (unique), summary, markdown, tag_ids (INT[]), private (bool),
  created_at, created_by (FK), modified_at, modified_by (FK)

page_revisions
  id (PK), page_id (FK), patch, created_at, created_by (FK)

images
  id (PK), title, description, data (bytea), thumbnail (bytea),
  created_at, created_by (FK)

galleries
  id (PK), title, description, image_ids (INT[]),
  created_at, created_by (FK)
```

## Routes

### Public

| Path | Method | Description |
|---|---|---|
| `/obrazky/{id}` | GET | Full image |
| `/obrazky/{id}/nahled` | GET | Thumbnail |
| `/{*path}` | GET | Catch-all: 1) menu match → render menu markdown, 2) page match → render page, 3) 404 |

### Admin

| Path | Method | Description |
|---|---|---|
| `/admin/login` | GET/POST | Login form |
| `/admin/logout` | GET | Logout |
| `/admin` | GET | Dashboard (protected) |
| `/admin/stranky` | CRUD | Page management |
| `/admin/menu` | CRUD | Menu management |
| `/admin/tagy` | CRUD | Tag management |
| `/admin/obrazky` | CRUD | Image management |
| `/admin/galerie` | CRUD | Gallery management |
| `/admin/tokeny` | CRUD | Service token management (for MCP) |

## MCP Server (Claude Code Integration)

The site exposes an MCP (Model Context Protocol) server at `/mcp/*` routes, allowing Claude Code to read and edit pages via tools.

### How Tool Descriptions Reach Claude

All MCP metadata is defined in `src/routes/mcp.rs`:

| Constant / Field | Location | What Claude Sees |
|---|---|---|
| `SERVER_INSTRUCTIONS` | line ~149 | Top-level server description in system reminders — explains page model, markdown extensions, and how to use tools |
| Tool `"description"` | `handle_tools_list()`, line ~190 | Per-tool summary shown when Claude discovers available tools |
| Property `"description"` | Inside each tool's `inputSchema` | Parameter-level hints Claude uses when calling a tool |

### Tuning Descriptions

To change what Claude sees about your site tools, edit these strings directly in `src/routes/mcp.rs`:

- **`SERVER_INSTRUCTIONS`** — high-level guidance: what a page is, what markdown extensions exist, how tools relate to each other. Keep this concise; Claude reads it on every conversation.
- **Tool `description`** — one sentence per tool explaining what it does and returns. Claude uses this to decide *which* tool to call.
- **Parameter `description`** — explains each parameter's purpose, format, and defaults. Claude uses these to fill in correct values.

### Tools

| Tool | Description |
|---|---|
| `read_page` | Read a page by exact path — returns metadata + markdown |
| `edit_page` | Create or update a page. Only provided fields change. Stores revision diffs automatically |
| `search_pages` | Filter pages by path prefix and/or tag name |
| `list_tags` | List all tags (name + description) |

### Auth

MCP routes use service tokens (created via `/admin/tokeny`). The token is sent as a Bearer token in the `Authorization` header. The MCP handler resolves the token to a user ID for audit fields (`created_by`, `modified_by`).

## Docker

```bash
# Build release binary first, then:
docker build -t site .
docker run -e DATABASE_URL=... -p 3000:3000 site
```

## Conventions

- Migrations auto-run on server startup
- Admin routes protected via Bearer token middleware
- Page revisions store diffs (patches), not full snapshots
- Images stored as binary in DB with auto-generated thumbnails
- Service tokens have no expiration by default (user manages lifecycle)
- Always run `cargo check` after changes to verify compilation
