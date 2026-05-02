//! Markdown rendering with `::tag` directives.
//!
//! Block- and inline-level directives use the `::name{key=value, key=value}`
//! syntax. Each directive resolves to HTML (or, for `::page`, to markdown
//! that is recursively re-scanned) before the markdown parser runs.
//!
//! ```text
//! ::page{path=infra/desktop/syncthing}
//! ::page{id=7}
//! ::file{path=spec.pdf}
//! ::file{id=42}
//! ::file{hash=ab12...}
//! ::img{path=diagram.png}
//! ::gallery{id=3}
//! ::gallery{path=holiday-2024}
//! ::fen{path=opening.fen, size=large}
//! ::pgn{hash=ab12..., move=12, size=small}
//! ```
//!
//! Argument values may be unquoted, single-quoted, or double-quoted; quoted
//! values support `\\` escapes. Multiple args are comma-separated.
//!
//! Lookup keys: exactly one of
//! - file-based (`::file`/`::img`/`::fen`/`::pgn`): `path`, `id`, or `hash` (sha256)
//! - `::gallery`: `path` or `id`
//! - `::page`: `path` or `id`
//!
//! Directives inside fenced code blocks (` ``` `, `~~~`) and inline code spans
//! (`` ` ``) are passed through verbatim.
//!
//! Each directive returns an HTML block buffered into a placeholder so the
//! markdown parser does not mangle it. Placeholders are restored after parsing.

/// Human-readable summary of the custom markdown directives. Shared by the MCP
/// server instructions and the assistant system prompt so AI tools know the
/// exact syntax they should produce.
pub const MARKDOWN_EXTENSIONS_DOC: &str = "\
Directives use the `::name{key=value, key=value}` syntax. Lookup keys:
- file-based (`::file`/`::img`/`::fen`/`::pgn`): exactly one of `path`, `id`, or `hash` (sha256)
- `::gallery`: exactly one of `path` or `id`
- `::page`: exactly one of `path` or `id`

- `::page{path=section/sub/page}` / `::page{id=N}` — transclude another page's rendered content inline.
- `::file{path|id|hash=...}` — embeds an image (if mime image/*) or a download link otherwise.
- `::img{path|id|hash=..., alt=...}` — force image embed (with link to full size and caption).
- `::gallery{path|id=...}` — embeds a gallery grid of thumbnails.
- `::fen{path|id|hash=..., size=small|large}` — renders a static chess board position.
- `::pgn{path|id|hash=..., move=N, size=small|large}` — renders a playable chess game viewer.
- Internal links `[Text](Path/To/Page.md)` are auto-rewritten to lowercase absolute paths.";

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;

use minijinja::{Environment, context};
use pulldown_cmark::{Options, Parser, html};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::entity::file as file_entity;
use crate::repo::files::title_from_path;
use crate::entity::gallery as gallery_entity;
use crate::entity::page as page_entity;
use crate::files;

struct RenderCtx<'a> {
    db: &'a DatabaseConnection,
    tmpl: &'a Environment<'static>,
    logged_in: bool,
    /// Pages already on the transclusion stack — prevents infinite recursion.
    visited_pages: HashSet<String>,
    /// HTML blocks held back from the markdown parser, keyed by index.
    placeholders: Vec<String>,
}

pub async fn render(
    md: &str,
    db: &DatabaseConnection,
    tmpl: &Environment<'static>,
    logged_in: bool,
) -> String {
    let mut ctx = RenderCtx {
        db,
        tmpl,
        logged_in,
        visited_pages: HashSet::new(),
        placeholders: Vec::new(),
    };

    let expanded = expand_directives(md, &mut ctx).await;

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(&expanded, opts);
    let mut out = String::new();
    html::push_html(&mut out, parser);

    // Reverse order: a parked block created by a transcluding directive
    // contains markers for the inner directives parked during its recursive
    // expansion. Those inner placeholders always have lower indices, so
    // substituting outer-first lets the inner markers surface in `out` before
    // we try to replace them.
    for (i, html_block) in ctx.placeholders.iter().enumerate().rev() {
        let placeholder = format!("<!--SITE_PLACEHOLDER_{i}-->");
        out = out.replace(&placeholder, html_block);
    }

    rewrite_internal_links(&out)
}

// ---------------------------------------------------------------------------
// Directive parsing
// ---------------------------------------------------------------------------

struct Directive {
    name: String,
    args: HashMap<String, String>,
}

impl Directive {
    fn arg(&self, key: &str) -> Option<&str> {
        self.args.get(key).map(String::as_str)
    }
}

/// Parse a directive starting at the beginning of `s`. Returns the parsed
/// directive and the number of bytes consumed.
fn parse_inline_directive(s: &str) -> Option<(Directive, usize)> {
    let rest = s.strip_prefix("::")?;

    let name_end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    if name_end == 0 {
        return None;
    }
    let name = rest[..name_end].to_owned();

    let after_name = &rest[name_end..];
    let (args, args_bytes) = if let Some(after_brace) = after_name.strip_prefix('{') {
        let close = find_args_close(after_brace)?;
        let inner = &after_brace[..close];
        // +2 for the surrounding braces
        (parse_args(inner), close + 2)
    } else {
        (HashMap::new(), 0)
    };

    let consumed = 2 + name_end + args_bytes;
    Some((Directive { name, args }, consumed))
}

/// Locate the matching `}` that closes a directive args block, respecting
/// quoted values so a `}` inside `path="a}b"` is ignored.
fn find_args_close(s: &str) -> Option<usize> {
    let mut chars = s.char_indices();
    let mut depth = 1usize;
    let mut quote: Option<char> = None;
    while let Some((i, c)) = chars.next() {
        match (quote, c) {
            (Some(_), '\\') => {
                chars.next();
            }
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"') | (None, '\'') => quote = Some(c),
            (None, '{') => depth += 1,
            (None, '}') => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            (None, _) => {}
        }
    }
    None
}

/// Parse `key=value, key="value with spaces", key='value'` argument lists.
fn parse_args(input: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut chars = input.chars().peekable();

    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace() || *c == ',') {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }

        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' || c == ',' || c.is_whitespace() {
                break;
            }
            key.push(c);
            chars.next();
        }

        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }

        if chars.peek() != Some(&'=') {
            if !key.is_empty() {
                out.insert(key, String::new());
            }
            continue;
        }
        chars.next();

        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }

        let mut value = String::new();
        match chars.peek().copied() {
            Some(quote @ ('"' | '\'')) => {
                chars.next();
                while let Some(c) = chars.next() {
                    if c == quote {
                        break;
                    } else if c == '\\' {
                        if let Some(esc) = chars.next() {
                            value.push(esc);
                        }
                    } else {
                        value.push(c);
                    }
                }
            }
            Some(_) => {
                while let Some(&c) = chars.peek() {
                    if c == ',' || c.is_whitespace() {
                        break;
                    }
                    value.push(c);
                    chars.next();
                }
            }
            None => {}
        }

        if !key.is_empty() {
            out.insert(key, value);
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Expansion: walk lines, skip code, dispatch directives
// ---------------------------------------------------------------------------

fn expand_directives<'a>(
    md: &'a str,
    ctx: &'a mut RenderCtx<'_>,
) -> Pin<Box<dyn Future<Output = String> + Send + 'a>> {
    Box::pin(expand_directives_impl(md, ctx))
}

async fn expand_directives_impl(md: &str, ctx: &mut RenderCtx<'_>) -> String {
    let mut out = String::new();
    let mut in_fence = false;
    let mut fence_char = '`';

    for raw_line in md.split_inclusive('\n') {
        let stripped = raw_line.trim_end_matches(['\n', '\r']);
        let trimmed = stripped.trim_start();

        if in_fence {
            out.push_str(raw_line);
            let close = if fence_char == '`' { "```" } else { "~~~" };
            if trimmed.starts_with(close) {
                in_fence = false;
            }
            continue;
        }
        if trimmed.starts_with("```") {
            in_fence = true;
            fence_char = '`';
            out.push_str(raw_line);
            continue;
        }
        if trimmed.starts_with("~~~") {
            in_fence = true;
            fence_char = '~';
            out.push_str(raw_line);
            continue;
        }

        let trailing = &raw_line[stripped.len()..];
        let expanded = expand_line_directives(stripped, ctx).await;
        out.push_str(&expanded);
        out.push_str(trailing);
    }

    out
}

/// Walk a line and expand any `::name{...}` directive outside inline code
/// spans. Backtick spans are passed through verbatim.
async fn expand_line_directives(line: &str, ctx: &mut RenderCtx<'_>) -> String {
    let mut out = String::new();
    let mut rest = line;

    while !rest.is_empty() {
        if rest.starts_with('`') {
            let tick_count = rest.bytes().take_while(|&b| b == b'`').count();
            let ticks = &rest[..tick_count];
            if let Some(close) = rest[tick_count..].find(ticks) {
                let end = tick_count + close + tick_count;
                out.push_str(&rest[..end]);
                rest = &rest[end..];
            } else {
                out.push_str(rest);
                rest = "";
            }
        } else if rest.starts_with("::") {
            if let Some((d, consumed)) = parse_inline_directive(rest) {
                let expansion = dispatch_directive(&d, ctx).await;
                out.push_str(&expansion);
                rest = &rest[consumed..];
            } else {
                out.push_str("::");
                rest = &rest[2..];
            }
        } else {
            let next = rest.find(['`', ':']).unwrap_or(rest.len());
            if next == 0 {
                let ch = rest.chars().next().unwrap();
                out.push(ch);
                rest = &rest[ch.len_utf8()..];
            } else {
                out.push_str(&rest[..next]);
                rest = &rest[next..];
            }
        }
    }

    out
}

async fn dispatch_directive(d: &Directive, ctx: &mut RenderCtx<'_>) -> String {
    match d.name.as_str() {
        "page" => directive_page(d, ctx).await,
        "file" => directive_file(d, ctx).await,
        "img" => directive_img(d, ctx).await,
        "gallery" => directive_gallery(d, ctx).await,
        "fen" => directive_fen(d, ctx).await,
        "pgn" => directive_pgn(d, ctx).await,
        unknown => format!("\n\n*[unknown directive `::{unknown}`]*\n\n"),
    }
}

/// Park `html` in the placeholder buffer and return the marker token. The
/// surrounding blank lines make the marker survive markdown's block parser.
fn park(ctx: &mut RenderCtx<'_>, html: String) -> String {
    let idx = ctx.placeholders.len();
    ctx.placeholders.push(html);
    format!("\n\n<!--SITE_PLACEHOLDER_{idx}-->\n\n")
}

/// Render a `markdown/<name>.html` template; on failure, log and emit a
/// visible inline error so authors can spot it.
fn render_md_template(
    ctx: &RenderCtx<'_>,
    name: &str,
    tctx: minijinja::value::Value,
) -> String {
    let path = format!("markdown/{name}.html");
    match ctx.tmpl.get_template(&path) {
        Ok(t) => match t.render(tctx) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(template = %path, error = %e, "markdown template render failed");
                format!("<p><em>[template `{path}` render failed]</em></p>")
            }
        },
        Err(e) => {
            tracing::error!(template = %path, error = %e, "markdown template missing");
            format!("<p><em>[template `{path}` missing]</em></p>")
        }
    }
}

// ---------------------------------------------------------------------------
// File / gallery lookup
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum FileLookup {
    Path(String),
    Id(i32),
    Hash(String),
}

fn parse_file_lookup(d: &Directive, name: &str) -> Result<FileLookup, String> {
    let path = d.arg("path").filter(|s| !s.is_empty());
    let id = d.arg("id").filter(|s| !s.is_empty());
    let hash = d.arg("hash").filter(|s| !s.is_empty());

    let count = path.is_some() as u8 + id.is_some() as u8 + hash.is_some() as u8;
    match count {
        0 => Err(format!(
            "\n\n*[`::{name}` requires `path`, `id`, or `hash`]*\n\n"
        )),
        1 => {
            if let Some(p) = path {
                Ok(FileLookup::Path(p.to_owned()))
            } else if let Some(i) = id {
                let n: i32 = i.parse().map_err(|_| {
                    format!("\n\n*[`::{name}` got invalid `id` (expected integer)]*\n\n")
                })?;
                Ok(FileLookup::Id(n))
            } else {
                let h = hash.unwrap().to_ascii_lowercase();
                if h.len() != 64 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Err(format!(
                        "\n\n*[`::{name}` got invalid `hash` (expected 64 hex chars)]*\n\n"
                    ));
                }
                Ok(FileLookup::Hash(h))
            }
        }
        _ => Err(format!(
            "\n\n*[`::{name}` accepts only one of `path`, `id`, `hash`]*\n\n"
        )),
    }
}

async fn fetch_file(db: &DatabaseConnection, lookup: &FileLookup) -> Option<file_entity::Model> {
    match lookup {
        FileLookup::Id(id) => file_entity::Entity::find_by_id(*id).one(db).await.ok().flatten(),
        FileLookup::Hash(h) => file_entity::Entity::find()
            .filter(file_entity::Column::Hash.eq(h.as_str()))
            .one(db)
            .await
            .ok()
            .flatten(),
        FileLookup::Path(p) => file_entity::Entity::find()
            .filter(file_entity::Column::Path.eq(crate::path_util::normalize(p)))
            .one(db)
            .await
            .ok()
            .flatten(),
    }
}

fn lookup_label(lookup: &FileLookup) -> String {
    match lookup {
        FileLookup::Path(p) => p.clone(),
        FileLookup::Id(i) => i.to_string(),
        FileLookup::Hash(h) => h.clone(),
    }
}

#[derive(Debug)]
enum GalleryLookup {
    Path(String),
    Id(i32),
}

fn parse_gallery_lookup(d: &Directive) -> Result<GalleryLookup, String> {
    let path = d.arg("path").filter(|s| !s.is_empty());
    let id = d.arg("id").filter(|s| !s.is_empty());

    match (path, id) {
        (Some(p), None) => Ok(GalleryLookup::Path(p.to_owned())),
        (None, Some(i)) => {
            let n: i32 = i.parse().map_err(|_| {
                "\n\n*[`::gallery` got invalid `id` (expected integer)]*\n\n".to_owned()
            })?;
            Ok(GalleryLookup::Id(n))
        }
        (Some(_), Some(_)) => {
            Err("\n\n*[`::gallery` accepts only one of `path`, `id`]*\n\n".to_owned())
        }
        (None, None) => Err("\n\n*[`::gallery` requires `path` or `id`]*\n\n".to_owned()),
    }
}

#[derive(Debug)]
enum PageLookup {
    Path(String),
    Id(i32),
}

fn parse_page_lookup(d: &Directive) -> Result<PageLookup, String> {
    let path = d.arg("path").filter(|s| !s.is_empty());
    let id = d.arg("id").filter(|s| !s.is_empty());

    match (path, id) {
        (Some(p), None) => Ok(PageLookup::Path(p.to_owned())),
        (None, Some(i)) => {
            let n: i32 = i.parse().map_err(|_| {
                "\n\n*[`::page` got invalid `id` (expected integer)]*\n\n".to_owned()
            })?;
            Ok(PageLookup::Id(n))
        }
        (Some(_), Some(_)) => {
            Err("\n\n*[`::page` accepts only one of `path`, `id`]*\n\n".to_owned())
        }
        (None, None) => Err("\n\n*[`::page` requires `path` or `id`]*\n\n".to_owned()),
    }
}

async fn fetch_page(db: &DatabaseConnection, lookup: &PageLookup) -> Option<page_entity::Model> {
    match lookup {
        PageLookup::Id(id) => page_entity::Entity::find_by_id(*id).one(db).await.ok().flatten(),
        PageLookup::Path(p) => page_entity::Entity::find()
            .filter(page_entity::Column::Path.eq(crate::path_util::normalize(p)))
            .one(db)
            .await
            .ok()
            .flatten(),
    }
}

async fn fetch_gallery(
    db: &DatabaseConnection,
    lookup: &GalleryLookup,
) -> Option<gallery_entity::Model> {
    match lookup {
        GalleryLookup::Id(id) => gallery_entity::Entity::find_by_id(*id)
            .one(db)
            .await
            .ok()
            .flatten(),
        GalleryLookup::Path(p) => gallery_entity::Entity::find()
            .filter(gallery_entity::Column::Path.eq(crate::path_util::normalize(p)))
            .one(db)
            .await
            .ok()
            .flatten(),
    }
}

// ---------------------------------------------------------------------------
// ::page{path=...}
// ---------------------------------------------------------------------------

async fn directive_page(d: &Directive, ctx: &mut RenderCtx<'_>) -> String {
    let lookup = match parse_page_lookup(d) {
        Ok(l) => l,
        Err(msg) => return msg,
    };

    let Some(page) = fetch_page(ctx.db, &lookup).await else {
        let label = match &lookup {
            PageLookup::Id(i) => i.to_string(),
            PageLookup::Path(p) => p.clone(),
        };
        let html = format!(r#"<p><em>[page "{label}" not found]</em></p>"#);
        return park(ctx, html);
    };

    if page.private && !ctx.logged_in {
        return String::new();
    }

    let path = page.path.clone();
    if ctx.visited_pages.contains(&path) {
        let html = format!(r#"<p><em>[recursive transclusion of "{path}" skipped]</em></p>"#);
        return park(ctx, html);
    }

    ctx.visited_pages.insert(path.clone());
    let nested = expand_directives(&page.markdown, ctx).await;
    ctx.visited_pages.remove(&path);

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(&nested, opts);
    let mut inner_html = String::new();
    html::push_html(&mut inner_html, parser);

    let html = render_md_template(
        ctx,
        "page",
        context! { path => &path, inner_html => &inner_html },
    );
    park(ctx, html)
}

// ---------------------------------------------------------------------------
// ::file{path|id|hash=...}  — image if mime image/*, else download link
// ---------------------------------------------------------------------------

async fn directive_file(d: &Directive, ctx: &mut RenderCtx<'_>) -> String {
    let lookup = match parse_file_lookup(d, "file") {
        Ok(l) => l,
        Err(msg) => return msg,
    };

    let Some(file) = fetch_file(ctx.db, &lookup).await else {
        let label = lookup_label(&lookup);
        let html = format!(r#"<p><em>[file "{label}" not found]</em></p>"#);
        return park(ctx, html);
    };

    let title = title_from_path(&file.path);
    if file.mimetype.starts_with("image/") {
        let html = render_md_template(
            ctx,
            "img",
            context! { hash => &file.hash, title => &title, alt => &title },
        );
        return park(ctx, html);
    }

    let description = file
        .description
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(title.as_str());
    let html = render_md_template(
        ctx,
        "file",
        context! { hash => &file.hash, title => &title, description },
    );
    park(ctx, html)
}

// ---------------------------------------------------------------------------
// ::img{path|id|hash=..., alt=...}  — force image embed
// ---------------------------------------------------------------------------

async fn directive_img(d: &Directive, ctx: &mut RenderCtx<'_>) -> String {
    let lookup = match parse_file_lookup(d, "img") {
        Ok(l) => l,
        Err(msg) => return msg,
    };

    let Some(file) = fetch_file(ctx.db, &lookup).await else {
        let label = lookup_label(&lookup);
        let html = format!(r#"<p><em>[image "{label}" not found]</em></p>"#);
        return park(ctx, html);
    };

    let title = title_from_path(&file.path);
    let alt = d
        .arg("alt")
        .filter(|s| !s.is_empty())
        .unwrap_or(title.as_str());
    let html = render_md_template(
        ctx,
        "img",
        context! { hash => &file.hash, title => &title, alt },
    );
    park(ctx, html)
}

// ---------------------------------------------------------------------------
// ::gallery{id|path=...}
// ---------------------------------------------------------------------------

async fn directive_gallery(d: &Directive, ctx: &mut RenderCtx<'_>) -> String {
    let lookup = match parse_gallery_lookup(d) {
        Ok(l) => l,
        Err(msg) => return msg,
    };

    let Some(gal) = fetch_gallery(ctx.db, &lookup).await else {
        let label = match &lookup {
            GalleryLookup::Id(i) => i.to_string(),
            GalleryLookup::Path(p) => p.clone(),
        };
        let html = format!(r#"<p><em>[gallery "{label}" not found]</em></p>"#);
        return park(ctx, html);
    };

    #[derive(serde::Serialize)]
    struct GalleryItem {
        hash: String,
        title: String,
    }

    let mut items: Vec<GalleryItem> = Vec::with_capacity(gal.file_ids.len());
    for file_id in &gal.file_ids {
        if let Ok(Some(img)) = file_entity::Entity::find_by_id(*file_id).one(ctx.db).await {
            items.push(GalleryItem {
                hash: img.hash,
                title: title_from_path(&img.path),
            });
        }
    }

    let html = render_md_template(
        ctx,
        "gallery",
        context! { id => gal.id, title => &gal.title, items => &items },
    );
    park(ctx, html)
}

// ---------------------------------------------------------------------------
// ::fen{path|id|hash=..., size=small|large}
// FEN string is read from the file blob.
// ---------------------------------------------------------------------------

async fn directive_fen(d: &Directive, ctx: &mut RenderCtx<'_>) -> String {
    let lookup = match parse_file_lookup(d, "fen") {
        Ok(l) => l,
        Err(msg) => return msg,
    };
    let size_class = parse_size_class(d);

    let Some(file) = fetch_file(ctx.db, &lookup).await else {
        let label = lookup_label(&lookup);
        let html = format!(r#"<p><em>[fen file "{label}" not found]</em></p>"#);
        return park(ctx, html);
    };

    let Some(fen) = read_text_blob(ctx.db, &file.hash).await else {
        let html = format!(r#"<p><em>[fen blob for "{}" missing]</em></p>"#, file.path);
        return park(ctx, html);
    };

    let html = render_md_template(
        ctx,
        "fen",
        context! { fen => fen.trim(), size_class => size_class },
    );
    park(ctx, html)
}

// ---------------------------------------------------------------------------
// ::pgn{path|id|hash=..., size=small|large, move=N}
// PGN text is read from the file blob.
// ---------------------------------------------------------------------------

async fn directive_pgn(d: &Directive, ctx: &mut RenderCtx<'_>) -> String {
    let lookup = match parse_file_lookup(d, "pgn") {
        Ok(l) => l,
        Err(msg) => return msg,
    };
    let size_class = parse_size_class(d);
    let move_attr = d.arg("move").filter(|s| !s.is_empty());

    let Some(file) = fetch_file(ctx.db, &lookup).await else {
        let label = lookup_label(&lookup);
        let html = format!(r#"<p><em>[pgn file "{label}" not found]</em></p>"#);
        return park(ctx, html);
    };

    let Some(pgn) = read_text_blob(ctx.db, &file.hash).await else {
        let html = format!(r#"<p><em>[pgn blob for "{}" missing]</em></p>"#, file.path);
        return park(ctx, html);
    };

    let html = render_md_template(
        ctx,
        "pgn",
        context! {
            pgn => pgn.trim(),
            size_class => size_class,
            move => move_attr,
        },
    );
    park(ctx, html)
}

fn parse_size_class(d: &Directive) -> &'static str {
    match d.arg("size").unwrap_or("") {
        "small" | "sm" => " size-sm",
        "large" | "lg" => " size-lg",
        _ => "",
    }
}

async fn read_text_blob(db: &DatabaseConnection, hash: &str) -> Option<String> {
    let bytes = files::read_blob(db, hash).await.ok().flatten()?;
    String::from_utf8(bytes).ok()
}

// ---------------------------------------------------------------------------
// Internal links: rewrite relative href values to page paths
// [Syncthing](Infra/Desktop/Syncthing.md) → <a href="/infra/desktop/syncthing">
// ---------------------------------------------------------------------------

fn rewrite_internal_links(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(pos) = rest.find("href=\"") {
        out.push_str(&rest[..pos + 6]);
        rest = &rest[pos + 6..];
        let Some(end) = rest.find('"') else { break };
        let href = &rest[..end];
        if is_internal_link(href) {
            let path = href.strip_suffix(".md").unwrap_or(href).to_lowercase();
            out.push('/');
            out.push_str(&path);
        } else {
            out.push_str(href);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn is_internal_link(href: &str) -> bool {
    !href.is_empty()
        && !href.contains("://")
        && !href.starts_with('/')
        && !href.starts_with('#')
        && !href.starts_with("mailto:")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_block(line: &str) -> Option<Directive> {
        let trimmed = line.trim();
        let (d, n) = parse_inline_directive(trimmed)?;
        if n == trimmed.len() { Some(d) } else { None }
    }

    #[test]
    fn parse_page_path() {
        let d = parse_block("::page{path=infra/desktop/syncthing}").unwrap();
        assert_eq!(d.name, "page");
        assert_eq!(d.arg("path"), Some("infra/desktop/syncthing"));
    }

    #[test]
    fn parse_quoted_value() {
        let d = parse_block(r#"::page{path="my page/with spaces"}"#).unwrap();
        assert_eq!(d.arg("path"), Some("my page/with spaces"));
    }

    #[test]
    fn parse_multi_args() {
        let d = parse_block("::pgn{hash=ab12, size=large, move=12}").unwrap();
        assert_eq!(d.arg("hash"), Some("ab12"));
        assert_eq!(d.arg("size"), Some("large"));
        assert_eq!(d.arg("move"), Some("12"));
    }

    #[test]
    fn rejects_old_transclude() {
        assert!(parse_block("![[some/page]]").is_none());
    }

    #[test]
    fn rejects_inline_code_span() {
        assert!(parse_block("`::page{path=foo}`").is_none());
    }

    #[test]
    fn handles_quoted_brace() {
        let (d, n) = parse_inline_directive(r#"::page{path="a}b"} tail"#).unwrap();
        assert_eq!(d.arg("path"), Some("a}b"));
        assert_eq!(n, r#"::page{path="a}b"}"#.len());
    }

    fn make_dir(name: &str, args: &[(&str, &str)]) -> Directive {
        let mut map = HashMap::new();
        for (k, v) in args {
            map.insert((*k).to_owned(), (*v).to_owned());
        }
        Directive {
            name: name.to_owned(),
            args: map,
        }
    }

    #[test]
    fn file_lookup_id() {
        let d = make_dir("file", &[("id", "42")]);
        match parse_file_lookup(&d, "file").unwrap() {
            FileLookup::Id(n) => assert_eq!(n, 42),
            other => panic!("expected id, got {other:?}"),
        }
    }

    #[test]
    fn file_lookup_hash_lowercases() {
        let h = "A".repeat(64);
        let d = make_dir("img", &[("hash", &h)]);
        match parse_file_lookup(&d, "img").unwrap() {
            FileLookup::Hash(out) => assert_eq!(out, "a".repeat(64)),
            other => panic!("expected hash, got {other:?}"),
        }
    }

    #[test]
    fn file_lookup_rejects_invalid_hash() {
        let d = make_dir("img", &[("hash", "deadbeef")]);
        let err = parse_file_lookup(&d, "img").unwrap_err();
        assert!(err.contains("invalid `hash`"));
    }

    #[test]
    fn file_lookup_rejects_two_keys() {
        let d = make_dir("file", &[("path", "a"), ("id", "1")]);
        let err = parse_file_lookup(&d, "file").unwrap_err();
        assert!(err.contains("only one"));
    }

    #[test]
    fn file_lookup_rejects_empty() {
        let d = make_dir("file", &[]);
        let err = parse_file_lookup(&d, "file").unwrap_err();
        assert!(err.contains("requires"));
    }

    #[test]
    fn size_class_parsing() {
        let d = make_dir("pgn", &[("size", "large")]);
        assert_eq!(parse_size_class(&d), " size-lg");
        let d = make_dir("pgn", &[("size", "sm")]);
        assert_eq!(parse_size_class(&d), " size-sm");
        let d = make_dir("pgn", &[]);
        assert_eq!(parse_size_class(&d), "");
    }
}
