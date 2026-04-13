use axum::extract::{Path, Query, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use axum_extra::extract::CookieJar;
use minijinja::context;
use sea_orm::{ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};

use crate::auth;
use crate::entity::{page, tag};
use crate::routes::build_menu;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/{id}", get(by_tag))
}

use super::pages::PageView;

const PAGE_SIZE: u64 = 20;

#[derive(serde::Deserialize)]
pub struct Pagination {
    #[serde(default = "default_page")]
    pub page: u64,
}

fn default_page() -> u64 {
    1
}

pub async fn by_tag(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(tag_id): Path<i32>,
    Query(pagination): Query<Pagination>,
) -> Html<String> {
    let logged_in = auth::is_logged_in(&state, &jar).await.is_some();
    let nav = build_menu(&state.db).await;

    // Find the tag
    let tag_model = match tag::Entity::find_by_id(tag_id)
        .one(&state.db)
        .await
    {
        Ok(Some(t)) => t,
        _ => {
            let tmpl = state.tmpl.get_template("404.html").unwrap();
            return match tmpl.render(context! { menu => nav, logged_in }) {
                Ok(html) => Html(html),
                Err(_) => Html("<h1>Page not found</h1>".to_string()),
            };
        }
    };

    // Query pages that contain this tag_id in their tag_ids array
    let current_page = pagination.page.max(1);
    let mut query = page::Entity::find()
        .filter(
            Condition::all().add(
                sea_orm::sea_query::Expr::cust_with_values(
                    "tag_ids @> ARRAY[$1]::int[]",
                    [tag_model.id],
                ),
            ),
        )
        .order_by_desc(page::Column::ModifiedAt);

    // Hide private pages from unauthenticated users
    if !logged_in {
        query = query.filter(page::Column::Private.eq(false));
    }

    let paginator = query.paginate(&state.db, PAGE_SIZE);
    let total_pages = paginator.num_pages().await.unwrap_or(1).max(1);
    let pages = paginator
        .fetch_page(current_page - 1)
        .await
        .unwrap_or_default()
        .iter()
        .map(PageView::from)
        .collect::<Vec<_>>();

    let tmpl = match state.tmpl.get_template("tag_pages.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("<h1>Template error</h1><pre>{e}</pre>")),
    };

    match tmpl.render(context! {
        tag => tag_model,
        pages,
        current_page,
        total_pages,
        menu => nav,
        logged_in,
    }) {
        Ok(html) => Html(html),
        Err(e) => Html(format!("<h1>Render error</h1><pre>{e}</pre>")),
    }
}
