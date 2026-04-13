pub mod auth;
pub mod galleries;
pub mod images;
pub mod menu;
pub mod pages;
pub mod tags;
pub mod tokens;

use axum::response::{IntoResponse, Redirect};
use axum::routing::get;
use axum::Router;

use crate::state::AppState;

pub async fn dashboard() -> impl IntoResponse {
    Redirect::to("/admin/stranky")
}

pub fn router(state: AppState) -> Router<AppState> {
    let protected = Router::new()
        .route("/", get(dashboard))
        .nest("/stranky", pages::router())
        .nest("/menu", menu::router())
        .nest("/tagy", tags::router())
        .nest("/obrazky", images::router())
        .nest("/galerie", galleries::router())
        .nest("/tokeny", tokens::router())
        .layer(axum::middleware::from_fn_with_state(
            state,
            crate::auth::require_login,
        ));

    Router::new().merge(protected).merge(auth::router())
}
