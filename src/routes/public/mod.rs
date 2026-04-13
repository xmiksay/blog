pub mod images;
pub mod pages;
pub mod tags;

use axum::extract::{Request, State};
use axum::response::Html;
use axum_extra::extract::CookieJar;
use minijinja::context;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::entity::{menu, page, tag};
use crate::routes::build_menu;
use crate::state::AppState;
use crate::{auth, markdown};

/// Catch-all handler: menu -> page -> 404
pub async fn catch_all(
    State(state): State<AppState>,
    jar: CookieJar,
    req: Request,
) -> Html<String> {
    let path = req.uri().path().trim_start_matches('/').to_string();
    let logged_in = auth::is_logged_in(&state, &jar).await.is_some();
    let nav = build_menu(&state.db).await;

    // 1. Check menu table
    if let Ok(Some(menu_item)) = menu::Entity::find()
        .filter(menu::Column::Path.eq(&path))
        .one(&state.db)
        .await
    {
        let body_html = markdown::render(&menu_item.markdown, &state.db, logged_in).await;
        let menu_id = menu_item.id;
        let tmpl = state.tmpl.get_template("menu_page.html").unwrap();
        return match tmpl.render(context! { body_html, menu => nav, logged_in, menu_id }) {
            Ok(html) => Html(html),
            Err(e) => Html(format!("<h1>Render error</h1><pre>{e}</pre>")),
        };
    }

    // 2. Check page table
    if let Ok(Some(pg)) = page::Entity::find()
        .filter(page::Column::Path.eq(&path))
        .one(&state.db)
        .await
    {
        if pg.private && !logged_in {
            return render_404(&state, &nav, logged_in);
        }

        let body_html = markdown::render(&pg.markdown, &state.db, logged_in).await;
        let page_view = pages::PageView::from(&pg);

        let tags = tag::Entity::find()
            .filter(tag::Column::Id.is_in(pg.tag_ids.clone()))
            .all(&state.db)
            .await
            .unwrap_or_default();

        let tmpl = match state.tmpl.get_template("page_detail.html") {
            Ok(t) => t,
            Err(e) => return Html(format!("<h1>Template error</h1><pre>{e}</pre>")),
        };

        return match tmpl.render(context! { page => page_view, body_html, tags, menu => nav, logged_in }) {
            Ok(html) => Html(html),
            Err(e) => Html(format!("<h1>Render error</h1><pre>{e}</pre>")),
        };
    }

    // 3. 404
    render_404(&state, &nav, logged_in)
}

fn render_404(state: &AppState, menu: &[crate::routes::MenuItem], logged_in: bool) -> Html<String> {
    let tmpl = state.tmpl.get_template("404.html").unwrap();
    match tmpl.render(context! { menu, logged_in }) {
        Ok(html) => Html(html),
        Err(_) => Html("<h1>Page not found</h1>".to_string()),
    }
}
