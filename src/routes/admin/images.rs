use axum::extract::{DefaultBodyLimit, Extension, Multipart, Path, State};
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::Router;
use image::codecs::jpeg::JpegEncoder;
use image::ImageReader;
use minijinja::context;
use sea_orm::{ActiveModelTrait, EntityTrait, ModelTrait, QueryOrder, QuerySelect, Set};
use std::io::Cursor;

pub const MAX_UPLOAD_SIZE: usize = 50 * 1024 * 1024;

use crate::entity::image as image_entity;
use crate::routes::build_menu;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(upload))
        .route("/nahrat", get(upload_form))
        .route("/{id}/edit", get(edit_form))
        .route("/{id}", post(update))
        .route("/{id}/smazat", post(delete))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_SIZE))
}

const THUMBNAIL_MAX: u32 = 200;
const IMAGE_MAX: u32 = 1920;
const JPEG_QUALITY: u8 = 85;

#[derive(serde::Serialize)]
struct ImageView {
    id: i32,
    title: String,
    description: Option<String>,
    created_at: String,
}

#[derive(Debug, sea_orm::FromQueryResult)]
struct ImageListModel {
    id: i32,
    title: String,
    description: Option<String>,
    created_at: chrono::DateTime<chrono::FixedOffset>,
}

pub async fn list(State(state): State<AppState>) -> impl IntoResponse {
    let images: Vec<ImageView> = image_entity::Entity::find()
        .select_only()
        .column(image_entity::Column::Id)
        .column(image_entity::Column::Title)
        .column(image_entity::Column::Description)
        .column(image_entity::Column::CreatedAt)
        .order_by_desc(image_entity::Column::CreatedAt)
        .into_model::<ImageListModel>()
        .all(&state.db)
        .await
        .unwrap_or_default()
        .iter()
        .map(|m| ImageView {
            id: m.id,
            title: m.title.clone(),
            description: m.description.clone(),
            created_at: m.created_at.to_string(),
        })
        .collect();
    let menu = build_menu(&state.db, true).await;
    let tmpl = state.tmpl.get_template("admin/images.html").unwrap();
    Html(
        tmpl.render(context! { images, menu, logged_in => true })
            .unwrap(),
    )
}

pub async fn upload_form(State(state): State<AppState>) -> impl IntoResponse {
    let menu = build_menu(&state.db, true).await;
    let tmpl = state.tmpl.get_template("admin/image_upload.html").unwrap();
    Html(
        tmpl.render(context! { menu, logged_in => true })
            .unwrap(),
    )
}

pub async fn upload(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut title = String::new();
    let mut description = String::new();
    let mut files: Vec<Vec<u8>> = Vec::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "title" => title = field.text().await.unwrap_or_default(),
            "description" => description = field.text().await.unwrap_or_default(),
            "files" => {
                if let Ok(bytes) = field.bytes().await {
                    if !bytes.is_empty() {
                        files.push(bytes.to_vec());
                    }
                }
            }
            _ => {}
        }
    }

    for (i, file_bytes) in files.iter().enumerate() {
        let img_title = if files.len() == 1 {
            title.clone()
        } else {
            format!("{} ({})", title, i + 1)
        };

        match process_image(file_bytes) {
            Ok((jpeg_data, thumb_data)) => {
                let desc = if description.is_empty() {
                    None
                } else {
                    Some(description.clone())
                };
                image_entity::ActiveModel {
                    title: Set(img_title),
                    description: Set(desc),
                    data: Set(jpeg_data),
                    thumbnail: Set(thumb_data),
                    created_by: Set(user_id),
                    ..Default::default()
                }
                .insert(&state.db)
                .await
                .unwrap();
            }
            Err(e) => tracing::error!("Image processing error: {e}"),
        }
    }

    Redirect::to("/admin/obrazky")
}

pub async fn edit_form(State(state): State<AppState>, Path(id): Path<i32>) -> impl IntoResponse {
    let img = image_entity::Entity::find_by_id(id)
        .select_only()
        .column(image_entity::Column::Id)
        .column(image_entity::Column::Title)
        .column(image_entity::Column::Description)
        .column(image_entity::Column::CreatedAt)
        .into_model::<ImageListModel>()
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    let view = ImageView {
        id: img.id,
        title: img.title,
        description: img.description,
        created_at: img.created_at.to_string(),
    };
    let menu = build_menu(&state.db, true).await;
    let tmpl = state.tmpl.get_template("admin/image_edit.html").unwrap();
    Html(
        tmpl.render(context! { image => view, menu, logged_in => true })
            .unwrap(),
    )
}

#[derive(serde::Deserialize)]
pub struct ImageEditForm {
    pub title: String,
    pub description: Option<String>,
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    axum::extract::Form(form): axum::extract::Form<ImageEditForm>,
) -> impl IntoResponse {
    let model = image_entity::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    let mut active: image_entity::ActiveModel = model.into();
    active.title = Set(form.title);
    active.description = Set(form.description.filter(|s| !s.is_empty()));
    active.update(&state.db).await.unwrap();
    Redirect::to("/admin/obrazky")
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<i32>) -> impl IntoResponse {
    let model = image_entity::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    model.delete(&state.db).await.unwrap();
    Redirect::to("/admin/obrazky")
}

fn encode_jpeg(img: &image::DynamicImage) -> Result<Vec<u8>, String> {
    let mut buf = Cursor::new(Vec::new());
    let encoder = JpegEncoder::new_with_quality(&mut buf, JPEG_QUALITY);
    img.write_with_encoder(encoder).map_err(|e| e.to_string())?;
    Ok(buf.into_inner())
}

pub fn process_image(bytes: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let img = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;

    let resized = if img.width() > IMAGE_MAX || img.height() > IMAGE_MAX {
        img.resize(IMAGE_MAX, IMAGE_MAX, image::imageops::FilterType::Lanczos3)
    } else {
        img.clone()
    };

    let jpeg_data = encode_jpeg(&resized)?;
    let thumb = img.thumbnail(THUMBNAIL_MAX, THUMBNAIL_MAX);
    let thumb_data = encode_jpeg(&thumb)?;

    Ok((jpeg_data, thumb_data))
}
