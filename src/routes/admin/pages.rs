use axum::extract::{Extension, Form, Path, State};
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::Router;
use minijinja::context;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, ModelTrait, QueryFilter, QueryOrder, Set,
};

use crate::entity::{page, page_revision, tag};
use crate::routes::build_menu;
use crate::routes::public::pages::PageView;
use crate::routes::revision;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/novy", get(new_form))
        .route("/{id}/edit", get(edit_form))
        .route("/{id}", post(update))
        .route("/{id}/smazat", post(delete))
        .route("/{id}/revize/{rev_id}/obnovit", post(restore))
}

#[derive(serde::Deserialize)]
pub struct PageForm {
    pub path: String,
    pub summary: Option<String>,
    pub markdown: String,
    pub tag_ids: Option<String>,
    pub private: Option<String>,
}

impl PageForm {
    fn parse_tag_ids(&self) -> Vec<i32> {
        self.tag_ids
            .as_deref()
            .unwrap_or("")
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect()
    }

    fn is_private(&self) -> bool {
        self.private.as_deref() == Some("on")
    }
}

#[derive(serde::Deserialize)]
pub struct ListQuery {
    pub sort: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListQuery>,
) -> impl IntoResponse {
    let sort = query.sort.as_deref().unwrap_or("modified");
    let mut select = page::Entity::find();
    select = match sort {
        "path" => select.order_by_asc(page::Column::Path),
        "path_desc" => select.order_by_desc(page::Column::Path),
        "modified" => select.order_by_desc(page::Column::ModifiedAt),
        "modified_asc" => select.order_by_asc(page::Column::ModifiedAt),
        _ => select.order_by_desc(page::Column::ModifiedAt),
    };
    let pages: Vec<PageView> = select
        .all(&state.db)
        .await
        .unwrap_or_default()
        .iter()
        .map(PageView::from)
        .collect();
    let menu = build_menu(&state.db, true).await;
    let tmpl = state.tmpl.get_template("admin/pages.html").unwrap();
    Html(
        tmpl.render(context! { pages, menu, logged_in => true, sort })
            .unwrap(),
    )
}

pub async fn new_form(State(state): State<AppState>) -> impl IntoResponse {
    let tags = tag::Entity::find().all(&state.db).await.unwrap_or_default();
    let menu = build_menu(&state.db, true).await;
    let tmpl = state.tmpl.get_template("admin/page_form.html").unwrap();
    Html(
        tmpl.render(context! { menu, logged_in => true, page => (), tags })
            .unwrap(),
    )
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    Form(form): Form<PageForm>,
) -> impl IntoResponse {
    let now = chrono::Utc::now().fixed_offset();
    let tag_ids = form.parse_tag_ids();
    let private = form.is_private();
    let saved = page::ActiveModel {
        path: Set(form.path),
        summary: Set(form.summary.filter(|s| !s.is_empty())),
        markdown: Set(form.markdown),
        tag_ids: Set(tag_ids),
        private: Set(private),
        created_at: Set(now),
        created_by: Set(user_id),
        modified_at: Set(now),
        modified_by: Set(user_id),
        ..Default::default()
    }
    .insert(&state.db)
    .await
    .unwrap();

    Redirect::to(&format!("/{}", saved.path))
}

pub async fn edit_form(State(state): State<AppState>, Path(id): Path<i32>) -> Html<String> {
    let pg = match page::Entity::find_by_id(id).one(&state.db).await {
        Ok(Some(p)) => p,
        Ok(None) => return Html(format!("<h1>Page {id} not found</h1>")),
        Err(e) => return Html(format!("<h1>DB error</h1><pre>{e}</pre>")),
    };
    let view = PageView::from(&pg);
    let tags = tag::Entity::find().all(&state.db).await.unwrap_or_default();
    let page_markdown = pg.markdown.clone();

    let revisions: Vec<RevisionView> = page_revision::Entity::find()
        .filter(page_revision::Column::PageId.eq(id))
        .order_by_desc(page_revision::Column::CreatedAt)
        .all(&state.db)
        .await
        .unwrap_or_default()
        .iter()
        .map(RevisionView::from)
        .collect();

    let menu = build_menu(&state.db, true).await;
    let tmpl = state.tmpl.get_template("admin/page_form.html").unwrap();
    match tmpl.render(context! { page => view, page_markdown, tags, revisions, menu, logged_in => true }) {
        Ok(html) => Html(html),
        Err(e) => Html(format!("<h1>Render error</h1><pre>{e}</pre>")),
    }
}

pub async fn update(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    Path(id): Path<i32>,
    Form(form): Form<PageForm>,
) -> impl IntoResponse {
    let model = page::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();

    let now = chrono::Utc::now().fixed_offset();
    let tag_ids = form.parse_tag_ids();
    let private = form.is_private();
    let new_path = form.path.clone();
    let old_markdown = model.markdown.clone();

    let mut active: page::ActiveModel = model.into();
    active.path = Set(form.path);
    active.summary = Set(form.summary.filter(|s| !s.is_empty()));
    active.markdown = Set(form.markdown.clone());
    active.tag_ids = Set(tag_ids);
    active.private = Set(private);
    active.modified_at = Set(now);
    active.modified_by = Set(user_id);
    active.update(&state.db).await.unwrap();

    revision::create_revision_if_changed(&state.db, id, &old_markdown, &form.markdown, user_id)
        .await;

    Redirect::to(&format!("/{}", new_path))
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<i32>) -> impl IntoResponse {
    let model = page::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    model.delete(&state.db).await.unwrap();
    Redirect::to("/admin/stranky")
}

pub async fn restore(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    Path((id, rev_id)): Path<(i32, i32)>,
) -> impl IntoResponse {
    let model = page::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();

    let restored = match revision::reconstruct_at_revision(
        &state.db,
        id,
        rev_id,
        &model.markdown,
    )
    .await
    {
        Ok(content) => content,
        Err(e) => {
            tracing::error!("Failed to restore revision {rev_id}: {e}");
            return Redirect::to(&format!("/admin/stranky/{id}/edit"));
        }
    };

    let old_markdown = model.markdown.clone();
    let now = chrono::Utc::now().fixed_offset();
    let mut active: page::ActiveModel = model.into();
    active.markdown = Set(restored.clone());
    active.modified_at = Set(now);
    active.modified_by = Set(user_id);
    active.update(&state.db).await.unwrap();

    revision::create_revision_if_changed(&state.db, id, &old_markdown, &restored, user_id).await;

    Redirect::to(&format!("/admin/stranky/{id}/edit"))
}

#[derive(serde::Serialize)]
struct RevisionView {
    id: i32,
    patch: String,
    created_at: String,
}

impl From<&page_revision::Model> for RevisionView {
    fn from(r: &page_revision::Model) -> Self {
        Self {
            id: r.id,
            patch: r.patch.clone(),
            created_at: r.created_at.to_string(),
        }
    }
}
