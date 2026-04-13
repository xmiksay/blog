use axum::extract::{Form, Path, State};
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::Router;
use minijinja::context;
use sea_orm::{ActiveModelTrait, EntityTrait, ModelTrait, QueryOrder, Set};

use crate::entity::tag;
use crate::routes::build_menu;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}/edit", get(edit_form))
        .route("/{id}", post(update))
        .route("/{id}/smazat", post(delete))
}

#[derive(serde::Deserialize)]
pub struct TagForm {
    pub name: String,
    pub description: Option<String>,
}

pub async fn list(State(state): State<AppState>) -> impl IntoResponse {
    let tags = tag::Entity::find()
        .order_by_asc(tag::Column::Name)
        .all(&state.db)
        .await
        .unwrap_or_default();
    let menu = build_menu(&state.db, true).await;
    let tmpl = state.tmpl.get_template("admin/tags.html").unwrap();
    Html(
        tmpl.render(context! { tags, menu, logged_in => true })
            .unwrap(),
    )
}

pub async fn create(
    State(state): State<AppState>,
    Form(form): Form<TagForm>,
) -> impl IntoResponse {
    tag::ActiveModel {
        name: Set(form.name),
        description: Set(form.description.filter(|s| !s.is_empty())),
        ..Default::default()
    }
    .insert(&state.db)
    .await
    .unwrap();
    Redirect::to("/admin/tagy")
}

pub async fn edit_form(State(state): State<AppState>, Path(id): Path<i32>) -> impl IntoResponse {
    let item = tag::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    let menu = build_menu(&state.db, true).await;
    let tmpl = state.tmpl.get_template("admin/tag_form.html").unwrap();
    Html(
        tmpl.render(context! { tag => item, menu, logged_in => true })
            .unwrap(),
    )
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Form(form): Form<TagForm>,
) -> impl IntoResponse {
    let model = tag::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    let mut active: tag::ActiveModel = model.into();
    active.name = Set(form.name);
    active.description = Set(form.description.filter(|s| !s.is_empty()));
    active.update(&state.db).await.unwrap();
    Redirect::to("/admin/tagy")
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<i32>) -> impl IntoResponse {
    let model = tag::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    model.delete(&state.db).await.unwrap();
    Redirect::to("/admin/tagy")
}
