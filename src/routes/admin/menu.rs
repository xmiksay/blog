use axum::extract::{Form, Path, State};
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::Router;
use minijinja::context;
use sea_orm::{ActiveModelTrait, EntityTrait, ModelTrait, QueryOrder, Set};

use crate::entity::menu;
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
pub struct MenuForm {
    pub title: String,
    pub path: String,
    pub markdown: String,
    pub order_index: i32,
}

pub async fn list(State(state): State<AppState>) -> impl IntoResponse {
    let menu_items = menu::Entity::find()
        .order_by_asc(menu::Column::OrderIndex)
        .all(&state.db)
        .await
        .unwrap_or_default();
    let nav_menu = build_menu(&state.db).await;
    let tmpl = state.tmpl.get_template("admin/menu.html").unwrap();
    Html(
        tmpl.render(context! { menu_items, menu => nav_menu, logged_in => true })
            .unwrap(),
    )
}

pub async fn create(
    State(state): State<AppState>,
    Form(form): Form<MenuForm>,
) -> impl IntoResponse {
    menu::ActiveModel {
        title: Set(form.title),
        path: Set(form.path),
        markdown: Set(form.markdown),
        order_index: Set(form.order_index),
        ..Default::default()
    }
    .insert(&state.db)
    .await
    .unwrap();
    Redirect::to("/admin/menu")
}

pub async fn edit_form(State(state): State<AppState>, Path(id): Path<i32>) -> impl IntoResponse {
    let item = menu::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    let nav_menu = build_menu(&state.db).await;
    let tmpl = state.tmpl.get_template("admin/menu_form.html").unwrap();
    Html(
        tmpl.render(context! { item, menu => nav_menu, logged_in => true })
            .unwrap(),
    )
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Form(form): Form<MenuForm>,
) -> impl IntoResponse {
    let model = menu::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    let mut active: menu::ActiveModel = model.into();
    active.title = Set(form.title);
    active.path = Set(form.path);
    active.markdown = Set(form.markdown);
    active.order_index = Set(form.order_index);
    active.update(&state.db).await.unwrap();
    Redirect::to("/admin/menu")
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<i32>) -> impl IntoResponse {
    let model = menu::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    model.delete(&state.db).await.unwrap();
    Redirect::to("/admin/menu")
}
