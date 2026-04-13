# Personal Blog

Rust/Axum server-rendered personal blog with admin CMS for managing pages, images, galleries, and navigation.

## Requirements

- Rust (edition 2024)
- PostgreSQL
- Docker & Docker Compose (for containerized setup)

## Quick Start with Docker Compose

1. Build the release binaries:

```bash
cargo build --release
```

2. Start the application:

```bash
docker compose up --build -d
```

The app will be available at `http://localhost:3000`.

## Creating a User

With Docker Compose running:

```bash
docker compose exec app ./blog_cli create-user <username> <password>
```

### Without Docker

Set `DATABASE_URL` in `.env` or environment, then:

```bash
cargo run --bin blog_cli -- create-user <username> <password>
```

## Database Migrations

Migrations run automatically on server startup. To manage them manually:

```bash
# Apply all pending migrations
cargo run --bin blog_migration

# Rollback last migration
cargo run --bin blog_migration -- down

# Reset and reapply all
cargo run --bin blog_migration -- fresh

# Show migration status
cargo run --bin blog_migration -- status
```

With Docker Compose:

```bash
docker compose exec app ./blog_migration
```

## Environment Variables

| Variable | Description | Default |
|---|---|---|
| `DATABASE_URL` | PostgreSQL connection string | `postgres://blog:blog@localhost:5432/blog` |
| `RUST_LOG` | Log level filter | `blog=debug,tower_http=debug,info` |

## Development

```bash
# Run locally (requires DATABASE_URL)
cargo run --bin blog_server

# Check compilation
cargo check
```
