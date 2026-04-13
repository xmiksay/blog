use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;

use pulldown_cmark::{Options, Parser, html};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::entity::image as image_entity;
use crate::entity::page as page_entity;

/// Render context passed through recursive transclusion calls.
struct RenderCtx<'a> {
    db: &'a DatabaseConnection,
    logged_in: bool,
    /// Pages already on the transclusion stack — prevents infinite recursion.
    visited: HashSet<String>,
}

pub async fn render(md: &str, db: &DatabaseConnection, logged_in: bool) -> String {
    let mut ctx = RenderCtx {
        db,
        logged_in,
        visited: HashSet::new(),
    };
    render_inner(md, &mut ctx).await
}

fn render_inner<'a>(md: &'a str, ctx: &'a mut RenderCtx<'_>) -> Pin<Box<dyn Future<Output = String> + Send + 'a>> {
    Box::pin(render_inner_impl(md, ctx))
}

async fn render_inner_impl(md: &str, ctx: &mut RenderCtx<'_>) -> String {
    let mut placeholders: Vec<String> = Vec::new();

    let with_transclude = extract_transclude_tags(md, &mut placeholders, ctx).await;
    let with_img = extract_img_tags(&with_transclude, &mut placeholders, ctx.db).await;
    let with_gallery = extract_gallery_tags(&with_img, &mut placeholders, ctx.db).await;
    let with_fen = extract_fen_tags(&with_gallery, &mut placeholders);
    let with_pgn = extract_pgn_tags(&with_fen, &mut placeholders);

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(&with_pgn, opts);
    let mut out = String::new();
    html::push_html(&mut out, parser);

    for (i, html_block) in placeholders.iter().enumerate() {
        let placeholder = format!("<!--BLOG_PLACEHOLDER_{i}-->");
        out = out.replace(&placeholder, html_block);
    }

    rewrite_internal_links(&out)
}

// ---------------------------------------------------------------------------
// Transclusion: ![[page_path]]
// ---------------------------------------------------------------------------

async fn extract_transclude_tags(
    input: &str,
    placeholders: &mut Vec<String>,
    ctx: &mut RenderCtx<'_>,
) -> String {
    let mut out = String::new();
    let mut rest = input;
    while let Some(start) = rest.find("![[") {
        out.push_str(&rest[..start]);
        rest = &rest[start + 3..];
        if let Some(end) = rest.find("]]") {
            let page_path = rest[..end].trim();
            let idx = placeholders.len();
            let html_block = transclude_page(page_path, ctx).await;
            placeholders.push(html_block);
            out.push_str(&format!("<!--BLOG_PLACEHOLDER_{idx}-->"));
            rest = &rest[end + 2..];
        } else {
            // No closing ]] — emit the opening literally
            out.push_str("![[");
        }
    }
    out.push_str(rest);
    out
}

async fn transclude_page(path: &str, ctx: &mut RenderCtx<'_>) -> String {
    // Recursion guard
    if ctx.visited.contains(path) {
        return format!(
            r#"<p><em>[recursive transclusion of "{path}" skipped]</em></p>"#
        );
    }

    let page = match page_entity::Entity::find()
        .filter(page_entity::Column::Path.eq(path))
        .one(ctx.db)
        .await
    {
        Ok(Some(pg)) => pg,
        _ => return format!(r#"<p><em>[page "{path}" not found]</em></p>"#),
    };

    // Private pages hidden from unauthenticated viewers
    if page.private && !ctx.logged_in {
        return String::new();
    }

    ctx.visited.insert(path.to_owned());
    let rendered = render_inner(&page.markdown, ctx).await;
    ctx.visited.remove(path);

    format!(r#"<div class="transclude" data-page="{path}">{rendered}</div>"#)
}

// ---------------------------------------------------------------------------
// Images: [img ID]
// ---------------------------------------------------------------------------

async fn extract_img_tags(
    input: &str,
    placeholders: &mut Vec<String>,
    db: &DatabaseConnection,
) -> String {
    let mut out = String::new();
    let mut rest = input;
    while let Some(start) = rest.find("[img ") {
        out.push_str(&rest[..start]);
        rest = &rest[start + 5..];
        if let Some(end) = rest.find(']') {
            let id_str = rest[..end].trim();
            if let Ok(id) = id_str.parse::<i32>() {
                let idx = placeholders.len();
                let html_block = match image_entity::Entity::find_by_id(id).one(db).await {
                    Ok(Some(img)) => {
                        let alt = img.title.replace('"', "&quot;");
                        format!(
                            r#"<figure class="article-image"><a href="/obrazky/{id}"><img src="/obrazky/{id}" alt="{alt}" loading="lazy"></a><figcaption>{}</figcaption></figure>"#,
                            img.title
                        )
                    }
                    _ => format!(r#"<p><em>[image {id} not found]</em></p>"#),
                };
                placeholders.push(html_block);
                out.push_str(&format!("<!--BLOG_PLACEHOLDER_{idx}-->"));
            }
            rest = &rest[end + 1..];
        }
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------
// Galleries: [gallery ID]
// ---------------------------------------------------------------------------

async fn extract_gallery_tags(
    input: &str,
    placeholders: &mut Vec<String>,
    db: &DatabaseConnection,
) -> String {
    let mut out = String::new();
    let mut rest = input;
    while let Some(start) = rest.find("[gallery ") {
        out.push_str(&rest[..start]);
        rest = &rest[start + 9..];
        if let Some(end) = rest.find(']') {
            let id_str = rest[..end].trim();
            if let Ok(id) = id_str.parse::<i32>() {
                let idx = placeholders.len();
                let html_block = build_gallery_html(id, db).await;
                placeholders.push(html_block);
                out.push_str(&format!("<!--BLOG_PLACEHOLDER_{idx}-->"));
            }
            rest = &rest[end + 1..];
        }
    }
    out.push_str(rest);
    out
}

async fn build_gallery_html(id: i32, db: &DatabaseConnection) -> String {
    use crate::entity::gallery;

    let gal = match gallery::Entity::find_by_id(id).one(db).await {
        Ok(Some(g)) => g,
        _ => return format!(r#"<p><em>[gallery {id} not found]</em></p>"#),
    };

    if gal.image_ids.is_empty() {
        return format!(
            r#"<div class="gallery"><h3>{}</h3><p><em>Gallery is empty</em></p></div>"#,
            gal.title
        );
    }

    let mut items = String::new();
    for img_id in &gal.image_ids {
        if let Ok(Some(img)) = image_entity::Entity::find_by_id(*img_id).one(db).await {
            let alt = img.title.replace('"', "&quot;");
            items.push_str(&format!(
                r#"<a href="/obrazky/{}" class="gallery-item"><img src="/obrazky/{}/nahled" alt="{alt}" loading="lazy"><span>{}</span></a>"#,
                img.id, img.id, img.title
            ));
        }
    }

    format!(
        r#"<div class="gallery"><h3>{}</h3><div class="gallery-grid">{items}</div></div>"#,
        gal.title
    )
}

// ---------------------------------------------------------------------------
// Chess FEN: [fen FEN_STRING] or [fen small FEN_STRING] or [fen large FEN_STRING]
// Renders a static board position via data attribute (chess-viewer.js picks it up)
// ---------------------------------------------------------------------------

fn extract_fen_tags(input: &str, placeholders: &mut Vec<String>) -> String {
    let mut out = String::new();
    let mut rest = input;
    while let Some(start) = rest.find("[fen ") {
        out.push_str(&rest[..start]);
        rest = &rest[start + 5..];
        if let Some(end) = rest.find(']') {
            let content = rest[..end].trim();
            let (size_class, fen) = parse_size_prefix(content);
            let idx = placeholders.len();
            placeholders.push(format!(
                r#"<div class="fen-viewer{size_class}" data-fen="{fen}"></div>"#
            ));
            out.push_str(&format!("<!--BLOG_PLACEHOLDER_{idx}-->"));
            rest = &rest[end + 1..];
        }
    }
    out.push_str(rest);
    out
}

/// Parses an optional size prefix ("small"/"large") from the beginning of content.
/// Returns (css_class_suffix, remaining_content).
fn parse_size_prefix(content: &str) -> (String, &str) {
    if let Some(rest) = content.strip_prefix("small ") {
        (" size-sm".to_owned(), rest.trim())
    } else if let Some(rest) = content.strip_prefix("large ") {
        (" size-lg".to_owned(), rest.trim())
    } else {
        (String::new(), content)
    }
}

// ---------------------------------------------------------------------------
// Chess PGN: [pgn]PGN_TEXT[/pgn] or [pgn move=N]PGN_TEXT[/pgn]
// Renders a playable game viewer via data attributes (chess-viewer.js picks it up)
// ---------------------------------------------------------------------------

fn extract_pgn_tags(input: &str, placeholders: &mut Vec<String>) -> String {
    let mut out = String::new();
    let mut rest = input;
    while let Some(start) = rest.find("[pgn") {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(tag_end) = rest.find(']') else { break };
        let tag_content = rest[4..tag_end].trim();

        // Parse attributes: move=N, size=small|large
        let mut move_attr = None;
        let mut size_class = String::new();
        for part in tag_content.split_whitespace() {
            if let Some(val) = part.strip_prefix("move=") {
                move_attr = Some(val.trim_matches('"').trim_matches('\''));
            } else if let Some(val) = part.strip_prefix("size=") {
                size_class = match val.trim_matches('"').trim_matches('\'') {
                    "small" | "sm" => " size-sm".to_owned(),
                    "large" | "lg" => " size-lg".to_owned(),
                    _ => String::new(),
                };
            }
        }

        rest = &rest[tag_end + 1..];
        if let Some(end) = rest.find("[/pgn]") {
            let escaped = rest[..end]
                .trim()
                .replace('&', "&amp;")
                .replace('"', "&quot;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            let move_data = match move_attr {
                Some(m) => format!(r#" data-move="{m}""#),
                None => String::new(),
            };
            let idx = placeholders.len();
            placeholders.push(format!(
                r#"<div class="pgn-viewer{size_class}" data-pgn="{escaped}"{move_data}>
  <div class="board"></div>
  <div class="controls">
    <button type="button" class="btn-first">⏮</button>
    <button type="button" class="btn-prev">◀</button>
    <span class="move-info"></span>
    <button type="button" class="btn-next">▶</button>
    <button type="button" class="btn-last">⏭</button>
  </div>
</div>"#
            ));
            out.push_str(&format!("<!--BLOG_PLACEHOLDER_{idx}-->"));
            rest = &rest[end + 6..];
        }
    }
    out.push_str(rest);
    out
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
            let path = href
                .strip_suffix(".md")
                .unwrap_or(href)
                .to_lowercase();
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
