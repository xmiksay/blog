use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use rust_embed::{Embed, EmbeddedFile};

#[derive(Embed)]
#[folder = "assets"]
pub struct Assets;

pub fn load(namespace: &str, path: &str) -> Option<EmbeddedFile> {
    Assets::get(&format!("{namespace}/{path}"))
        .or_else(|| Assets::get(&format!("common/{path}")))
}

pub fn build_asset_response(path: &str, file: EmbeddedFile) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime.as_ref().to_string()),
            (
                header::CACHE_CONTROL,
                "public, max-age=86400".to_string(),
            ),
        ],
        file.data.to_vec(),
    )
        .into_response()
}
