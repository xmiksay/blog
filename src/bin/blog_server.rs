use axum::Router;
use axum::routing::get;
use blog::migration::{Migrator, MigratorTrait};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() {
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("blog=debug,tower_http=debug,info")),
        )
        .init();

    let state = blog::state::create_state().await;
    Migrator::up(&state.db, None)
        .await
        .expect("Migrations failed");

    use blog::routes::{admin, mcp, oauth, public};

    let app = Router::new()
        .merge(mcp::router())
        .merge(oauth::router())
        .nest("/obrazky", public::images::router())
        .nest("/tag", public::tags::router())
        .nest("/admin", admin::router(state.clone()))
        .nest_service("/static", ServeDir::new("static"))
        .layer(CatchPanicLayer::new())
        .layer(TraceLayer::new_for_http())
        .fallback(get(public::catch_all))
        .with_state(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tracing::info!("http://localhost:{port}");
    axum::serve(listener, app).await.unwrap();
}
