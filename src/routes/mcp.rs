use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::entity::{page, tag};
use crate::routes::{oauth, revision};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/mcp", post(handle))
}

const SERVER_NAME: &str = "blog";
const SERVER_VERSION: &str = "1.0.0";
const PROTOCOL_VERSION: &str = "2025-03-26";

// --- JSON-RPC types ---

#[derive(Deserialize)]
pub struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

impl JsonRpcResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

// --- Tool input types ---

#[derive(Deserialize)]
struct ReadPageArgs {
    path: String,
}

#[derive(Deserialize)]
struct EditPageArgs {
    path: String,
    markdown: Option<String>,
    summary: Option<String>,
    #[serde(default)]
    tag_names: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct SearchPagesArgs {
    #[serde(default)]
    prefix: Option<String>,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    limit: Option<u64>,
    #[serde(default)]
    offset: Option<u64>,
}

// --- MCP endpoint ---

pub async fn handle(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<JsonRpcRequest>,
) -> Response {
    let user_id = match oauth::authenticate_mcp(&state, &headers).await {
        Ok(uid) => uid,
        Err((status, www_auth)) => {
            let body = Json(JsonRpcResponse::error(None, -32000, "Unauthorized"));
            let mut response: Response = (status, body).into_response();
            if let Ok(val) = HeaderValue::from_str(&www_auth) {
                response.headers_mut().insert("WWW-Authenticate", val);
            }
            return response;
        }
    };

    let resp = match req.method.as_str() {
        "initialize" => handle_initialize(&state, req.id).await,
        "notifications/initialized" => {
            return (StatusCode::OK, Json(JsonRpcResponse::success(req.id, json!({})))).into_response();
        }
        "tools/list" => handle_tools_list(req.id),
        "tools/call" => handle_tools_call(&state, user_id, req.id.clone(), req.params).await,
        _ => JsonRpcResponse::error(req.id, -32601, format!("Method not found: {}", req.method)),
    };

    (StatusCode::OK, Json(resp)).into_response()
}

async fn handle_initialize(state: &AppState, id: Option<Value>) -> JsonRpcResponse {
    let instructions = match page::Entity::find()
        .filter(page::Column::Path.eq("CLAUDE"))
        .one(&state.db)
        .await
    {
        Ok(Some(p)) => p.markdown,
        _ => SERVER_INSTRUCTIONS.to_string(),
    };

    JsonRpcResponse::success(
        id,
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": SERVER_NAME,
                "version": SERVER_VERSION
            },
            "instructions": instructions
        }),
    )
}

const SERVER_INSTRUCTIONS: &str = "\
# Personal Blog — MCP Integration

Server-rendered blog. Pages are stored in PostgreSQL and served at their `path` \
(e.g. path `obsidian/work` → URL `/obsidian/work`).

## Pages

- **path**: unique URL slug. Hierarchical paths use `/` (e.g. `obsidian/programing/rust`).
- **markdown**: content in Markdown with custom extensions (see below).
- **summary**: short description for listings.
- **tags**: assigned by name via `edit_page`. Tags must already exist.
- **private**: private pages are only visible to logged-in users. \
  New pages created via MCP default to private.
- **revisions**: every markdown change stores a diff automatically.

## Markdown extensions

The blog renders standard Markdown plus these custom tags:

- `![[page_path]]` — **transclude**: embeds another page's rendered content inline. \
  Recursive transclusion is detected and skipped. Private pages are hidden from \
  unauthenticated viewers.
- `[img ID]` — embeds an image (with link to full size and caption).
- `[gallery ID]` — embeds a gallery grid of thumbnails.
- `[fen FEN_STRING]` — renders a static chess board position. \
  Optional size prefix: `[fen small FEN]` or `[fen large FEN]`.
- `[pgn]PGN_TEXT[/pgn]` — renders a playable chess game viewer with navigation controls. \
  Optional attributes: `[pgn move=5 size=small]PGN[/pgn]`.
- Internal links `[Text](Path/To/Page.md)` are auto-rewritten to lowercase absolute paths \
  (e.g. `href=\"/path/to/page\"`), so you can use either style.

## Working with pages

- `search_pages`: list/filter pages by path prefix and/or tag name.
- `read_page`: read a page by exact path — returns metadata + markdown.
- `edit_page`: create (new path) or update (existing path). Only provided fields change.
- `list_tags`: see available tags for filtering or assigning.
- Links between pages: `[Link text](/path/to/page)` or `[Text](Path/To/Page.md)`.
";

fn handle_tools_list(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(
        id,
        json!({
            "tools": [
                {
                    "name": "read_page",
                    "description": "Read a page by its path. Returns title (path), summary, tags, and full markdown content.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "The page path (e.g. 'infra/desktop/syncthing')"
                            }
                        },
                        "required": ["path"]
                    }
                },
                {
                    "name": "edit_page",
                    "description": "Create or update a page by its path. Creates the page if it doesn't exist. A revision diff is stored automatically when markdown changes.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "The page path to edit"
                            },
                            "markdown": {
                                "type": "string",
                                "description": "New markdown content (optional)"
                            },
                            "summary": {
                                "type": "string",
                                "description": "New summary (optional)"
                            },
                            "tag_names": {
                                "type": "array",
                                "items": { "type": "string" },
                                "description": "Tag names to assign (optional, replaces existing tags)"
                            }
                        },
                        "required": ["path"]
                    }
                },
                {
                    "name": "list_tags",
                    "description": "List all available tags. Returns tag name and description.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "search_pages",
                    "description": "Search pages by path prefix and/or tag name. Returns path, summary for each match, plus total count and has_more flag for pagination.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "prefix": {
                                "type": "string",
                                "description": "Path prefix to filter by (case-insensitive). If omitted, returns all pages."
                            },
                            "tag": {
                                "type": "string",
                                "description": "Optional tag name — only returns pages with this tag"
                            },
                            "limit": {
                                "type": "integer",
                                "description": "Max results to return (default 20, max 100)"
                            },
                            "offset": {
                                "type": "integer",
                                "description": "Number of results to skip for pagination (default 0)"
                            }
                        }
                    }
                }
            ]
        }),
    )
}

async fn handle_tools_call(
    state: &AppState,
    user_id: i32,
    id: Option<Value>,
    params: Option<Value>,
) -> JsonRpcResponse {
    let params = match params {
        Some(p) => p,
        None => return JsonRpcResponse::error(id, -32602, "Missing params"),
    };

    let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    match tool_name {
        "read_page" => tool_read_page(state, id, arguments).await,
        "edit_page" => tool_edit_page(state, user_id, id, arguments).await,
        "list_tags" => tool_list_tags(state, id).await,
        "search_pages" => tool_search_pages(state, id, arguments).await,
        _ => JsonRpcResponse::error(id, -32602, format!("Unknown tool: {tool_name}")),
    }
}

fn tool_result(id: Option<Value>, text: String) -> JsonRpcResponse {
    JsonRpcResponse::success(
        id,
        json!({
            "content": [{
                "type": "text",
                "text": text
            }]
        }),
    )
}

fn tool_error(id: Option<Value>, message: &str) -> JsonRpcResponse {
    JsonRpcResponse::success(
        id,
        json!({
            "isError": true,
            "content": [{
                "type": "text",
                "text": message
            }]
        }),
    )
}

// --- Tool implementations ---

async fn tool_read_page(
    state: &AppState,
    id: Option<Value>,
    arguments: Value,
) -> JsonRpcResponse {
    let args: ReadPageArgs = match serde_json::from_value(arguments) {
        Ok(a) => a,
        Err(e) => return tool_error(id, &format!("Invalid arguments: {e}")),
    };

    let pg = page::Entity::find()
        .filter(page::Column::Path.eq(&args.path))
        .one(&state.db)
        .await;

    match pg {
        Ok(Some(p)) => {
            let tag_names = resolve_tag_names(state, &p.tag_ids).await;
            let mut out = format!("# {}\n\n", p.path);
            if !tag_names.is_empty() {
                out.push_str(&format!("Tags: {}\n", tag_names.join(", ")));
            }
            if let Some(ref summary) = p.summary {
                out.push_str(&format!("Summary: {summary}\n"));
            }
            out.push_str(&format!("Modified: {}\n", p.modified_at));
            if p.private {
                out.push_str("Private: yes\n");
            }
            out.push_str("\n---\n");
            out.push_str(&p.markdown);
            tool_result(id, out)
        }
        Ok(None) => tool_error(id, &format!("Page not found: {}", args.path)),
        Err(e) => tool_error(id, &format!("Database error: {e}")),
    }
}

async fn tool_edit_page(
    state: &AppState,
    user_id: i32,
    id: Option<Value>,
    arguments: Value,
) -> JsonRpcResponse {
    let args: EditPageArgs = match serde_json::from_value(arguments) {
        Ok(a) => a,
        Err(e) => return tool_error(id, &format!("Invalid arguments: {e}")),
    };

    if args.markdown.is_none() && args.summary.is_none() && args.tag_names.is_none() {
        return tool_error(id, "Nothing to update — provide markdown, summary, or tag_names");
    }

    let now = chrono::Utc::now().fixed_offset();

    // Resolve tag names to IDs if provided
    let tag_ids = match &args.tag_names {
        Some(names) if !names.is_empty() => {
            match resolve_tag_ids(state, names).await {
                Ok(ids) => Some(ids),
                Err(e) => return tool_error(id, &e),
            }
        }
        _ => None,
    };

    let existing = page::Entity::find()
        .filter(page::Column::Path.eq(&args.path))
        .one(&state.db)
        .await;

    let existing = match existing {
        Ok(e) => e,
        Err(e) => return tool_error(id, &format!("Database error: {e}")),
    };

    let (action, old_markdown) = match existing {
        Some(model) => {
            let old_md = model.markdown.clone();
            let page_id = model.id;
            let mut active: page::ActiveModel = model.into();

            if let Some(ref markdown) = args.markdown {
                active.markdown = Set(markdown.clone());
            }
            if let Some(ref summary) = args.summary {
                active.summary = Set(Some(summary.clone()).filter(|s| !s.is_empty()));
            }
            if let Some(ref ids) = tag_ids {
                active.tag_ids = Set(ids.clone());
            }
            active.modified_at = Set(now);
            active.modified_by = Set(user_id);

            match active.update(&state.db).await {
                Ok(_) => (("updated", page_id), Some(old_md)),
                Err(e) => return tool_error(id, &format!("Update failed: {e}")),
            }
        }
        None => {
            let new_page = page::ActiveModel {
                path: Set(args.path.clone()),
                summary: Set(args.summary.clone().filter(|s| !s.is_empty())),
                markdown: Set(args.markdown.clone().unwrap_or_default()),
                tag_ids: Set(tag_ids.clone().unwrap_or_default()),
                private: Set(true),
                created_at: Set(now),
                created_by: Set(user_id),
                modified_at: Set(now),
                modified_by: Set(user_id),
                ..Default::default()
            };

            match new_page.insert(&state.db).await {
                Ok(saved) => (("created", saved.id), None),
                Err(e) => return tool_error(id, &format!("Create failed: {e}")),
            }
        }
    };

    let (status, page_id) = action;

    if let Some(ref new_markdown) = args.markdown {
        let old = old_markdown.as_deref().unwrap_or("");
        revision::create_revision_if_changed(&state.db, page_id, old, new_markdown, user_id)
            .await;
    }

    tool_result(id, format!("{status}: {}", args.path))
}

async fn tool_search_pages(
    state: &AppState,
    id: Option<Value>,
    arguments: Value,
) -> JsonRpcResponse {
    let args: SearchPagesArgs = match serde_json::from_value(arguments) {
        Ok(a) => a,
        Err(e) => return tool_error(id, &format!("Invalid arguments: {e}")),
    };

    // Build filter condition
    let mut condition = Condition::all();

    if let Some(prefix) = &args.prefix {
        if !prefix.is_empty() {
            condition = condition.add(page::Column::Path.starts_with(prefix));
        }
    }

    if let Some(tag_name) = &args.tag {
        if !tag_name.is_empty() {
            let tag_model = tag::Entity::find()
                .filter(tag::Column::Name.eq(tag_name.as_str()))
                .one(&state.db)
                .await;

            match tag_model {
                Ok(Some(t)) => {
                    condition = condition.add(sea_orm::sea_query::Expr::cust_with_values(
                        "? = ANY(tag_ids)",
                        [t.id],
                    ));
                }
                Ok(None) => {
                    return tool_result(id, "No pages found.".into());
                }
                Err(e) => return tool_error(id, &format!("Database error: {e}")),
            }
        }
    }

    let limit = args.limit.unwrap_or(20).min(100);
    let offset = args.offset.unwrap_or(0);

    let total = match page::Entity::find()
        .filter(condition.clone())
        .count(&state.db)
        .await
    {
        Ok(c) => c,
        Err(e) => return tool_error(id, &format!("Database error: {e}")),
    };

    if total == 0 {
        return tool_result(id, "No pages found.".into());
    }

    let pages: Vec<page::Model> = match page::Entity::find()
        .filter(condition)
        .order_by_desc(page::Column::ModifiedAt)
        .offset(offset)
        .limit(limit)
        .all(&state.db)
        .await
    {
        Ok(p) => p,
        Err(e) => return tool_error(id, &format!("Database error: {e}")),
    };

    let has_more = offset + limit < total;

    let mut out = pages
        .iter()
        .map(|p| match &p.summary {
            Some(s) if !s.is_empty() => format!("{}: {s}", p.path),
            _ => p.path.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n");

    out.push_str(&format!("\n\n--- total: {total}, has_more: {has_more}"));
    if has_more {
        out.push_str(&format!(", next_offset: {}", offset + limit));
    }
    out.push_str(" ---");

    tool_result(id, out)
}

async fn tool_list_tags(state: &AppState, id: Option<Value>) -> JsonRpcResponse {
    let tags = tag::Entity::find()
        .order_by_asc(tag::Column::Name)
        .all(&state.db)
        .await;

    match tags {
        Ok(tags) if tags.is_empty() => tool_result(id, "No tags defined.".into()),
        Ok(tags) => {
            let out = tags
                .iter()
                .map(|t| match &t.description {
                    Some(d) if !d.is_empty() => format!("{}: {d}", t.name),
                    _ => t.name.clone(),
                })
                .collect::<Vec<_>>()
                .join("\n");
            tool_result(id, out)
        }
        Err(e) => tool_error(id, &format!("Database error: {e}")),
    }
}

async fn resolve_tag_ids(state: &AppState, names: &[String]) -> Result<Vec<i32>, String> {
    let tags = tag::Entity::find()
        .filter(tag::Column::Name.is_in(names.iter().map(|s| s.as_str())))
        .all(&state.db)
        .await
        .map_err(|e| format!("Database error: {e}"))?;

    let found: Vec<String> = tags.iter().map(|t| t.name.clone()).collect();
    let missing: Vec<&String> = names.iter().filter(|n| !found.contains(n)).collect();
    if !missing.is_empty() {
        return Err(format!(
            "Unknown tags: {}",
            missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }

    Ok(tags.iter().map(|t| t.id).collect())
}

async fn resolve_tag_names(state: &AppState, tag_ids: &[i32]) -> Vec<String> {
    if tag_ids.is_empty() {
        return vec![];
    }
    tag::Entity::find()
        .filter(tag::Column::Id.is_in(tag_ids.iter().copied()))
        .all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|t| t.name)
        .collect()
}
