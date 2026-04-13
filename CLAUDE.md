# CLAUDE.md — Personal Blog

## Overview

Server-rendered personal blog. Rust/Axum backend with PostgreSQL, MiniJinja templates, and an admin CMS for managing pages, images, galleries, tags, and navigation menus. Auth via Bearer tokens; service tokens for MCP integration.

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
    blog_server.rs      # Main web server (port 3000)
    blog_migration.rs   # Migration CLI (up/down/fresh/status)
    blog_cli.rs         # Admin CLI (create-user)
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
cargo run --bin blog_server

# Migrations
cargo run --bin blog_migration          # apply all
cargo run --bin blog_migration -- down   # rollback last
cargo run --bin blog_migration -- fresh  # reset & reapply
cargo run --bin blog_migration -- status

# Create user
cargo run --bin blog_cli -- create-user <username> <password>
```

## Environment

- `DATABASE_URL` — PostgreSQL connection string (default: `postgres://blog:blog@localhost:5432/blog`)
- `RUST_LOG` — optional, defaults to `blog=debug,tower_http=debug,info`

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

## Docker

```bash
# Build release binary first, then:
docker build -t blog .
docker run -e DATABASE_URL=... -p 3000:3000 blog
```

## Conventions

- Migrations auto-run on server startup
- Admin routes protected via Bearer token middleware
- Page revisions store diffs (patches), not full snapshots
- Images stored as binary in DB with auto-generated thumbnails
- Service tokens have no expiration by default (user manages lifecycle)
- Always run `cargo check` after changes to verify compilation
