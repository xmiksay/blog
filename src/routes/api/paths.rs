use std::collections::BTreeMap;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::routing::post;
use serde::{Deserialize, Serialize};

use crate::path_util;
use crate::repo::files as files_repo;
use crate::repo::galleries as galleries_repo;
use crate::repo::pages as pages_repo;
use crate::routes::api::error::ApiResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/children", post(children))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Namespace {
    #[default]
    All,
    Page,
    Gallery,
    File,
}

#[derive(Deserialize)]
pub struct PathChildrenBody {
    #[serde(default)]
    pub namespace: Namespace,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub limit: Option<u64>,
}

#[derive(Serialize)]
pub struct PathChildrenResponse {
    pub prefix: String,
    pub folders: Vec<FolderEntry>,
    pub leaves: Vec<LeafEntry>,
}

#[derive(Serialize)]
pub struct FolderEntry {
    pub name: String,
    pub page_count: i64,
    pub gallery_count: i64,
    pub file_count: i64,
}

#[derive(Serialize)]
pub struct LeafEntry {
    pub name: String,
    pub namespace: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

pub async fn children(
    State(state): State<AppState>,
    Json(body): Json<PathChildrenBody>,
) -> ApiResult<Json<PathChildrenResponse>> {
    let prefix = path_util::normalize_prefix(&body.prefix);
    let limit = body.limit.unwrap_or(200).clamp(1, 1000);

    let want_pages = matches!(body.namespace, Namespace::All | Namespace::Page);
    let want_galleries = matches!(body.namespace, Namespace::All | Namespace::Gallery);
    let want_files = matches!(body.namespace, Namespace::All | Namespace::File);

    let page_rows = if want_pages {
        pages_repo::list_children(&state.db, &prefix, true, limit).await?
    } else {
        vec![]
    };
    let gallery_rows = if want_galleries {
        galleries_repo::list_children(&state.db, &prefix, limit).await?
    } else {
        vec![]
    };
    let file_rows = if want_files {
        files_repo::list_children(&state.db, &prefix, limit).await?
    } else {
        vec![]
    };

    let (folders, leaves) = merge(page_rows, gallery_rows, file_rows);
    Ok(Json(PathChildrenResponse {
        prefix,
        folders,
        leaves,
    }))
}

fn merge(
    pages: Vec<pages_repo::ChildRow>,
    galleries: Vec<pages_repo::ChildRow>,
    files: Vec<pages_repo::ChildRow>,
) -> (Vec<FolderEntry>, Vec<LeafEntry>) {
    let mut folders: BTreeMap<String, FolderEntry> = BTreeMap::new();
    let mut leaves: Vec<LeafEntry> = Vec::new();

    for r in pages {
        if r.has_descendants {
            folders
                .entry(r.name.clone())
                .and_modify(|f| f.page_count += r.descendant_count)
                .or_insert(FolderEntry {
                    name: r.name.clone(),
                    page_count: r.descendant_count,
                    gallery_count: 0,
                    file_count: 0,
                });
        }
        if r.has_leaf {
            leaves.push(LeafEntry {
                name: r.name,
                namespace: "page",
                title: r.leaf_title,
            });
        }
    }

    for r in galleries {
        if r.has_descendants {
            folders
                .entry(r.name.clone())
                .and_modify(|f| f.gallery_count += r.descendant_count)
                .or_insert(FolderEntry {
                    name: r.name.clone(),
                    page_count: 0,
                    gallery_count: r.descendant_count,
                    file_count: 0,
                });
        }
        if r.has_leaf {
            leaves.push(LeafEntry {
                name: r.name,
                namespace: "gallery",
                title: r.leaf_title,
            });
        }
    }

    for r in files {
        if r.has_descendants {
            folders
                .entry(r.name.clone())
                .and_modify(|f| f.file_count += r.descendant_count)
                .or_insert(FolderEntry {
                    name: r.name.clone(),
                    page_count: 0,
                    gallery_count: 0,
                    file_count: r.descendant_count,
                });
        }
        if r.has_leaf {
            leaves.push(LeafEntry {
                name: r.name,
                namespace: "file",
                title: r.leaf_title,
            });
        }
    }

    leaves.sort_by(|a, b| a.name.cmp(&b.name));
    (folders.into_values().collect(), leaves)
}
