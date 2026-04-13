use axum::extract::{DefaultBodyLimit, Extension, Form, Multipart, Path, State};
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::Router;
use minijinja::context;
use sea_orm::{ActiveModelTrait, EntityTrait, ModelTrait, QueryOrder, Set};

use crate::entity::{gallery, image as image_entity};
use crate::routes::admin::images::{process_image, MAX_UPLOAD_SIZE};
use crate::routes::build_menu;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/nova", get(new_form))
        .route("/{id}/edit", get(edit_form))
        .route("/{id}", post(update))
        .route("/{id}/smazat", post(delete))
        .route("/{id}/obrazky", post(add_image))
        .route("/{id}/nahrat", post(upload_images))
        .route("/{id}/obrazky/{image_id}/smazat", post(remove_image))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_SIZE))
}

#[derive(serde::Serialize)]
struct GalleryView {
    id: i32,
    title: String,
    description: Option<String>,
    image_count: usize,
    created_at: String,
}

#[derive(serde::Serialize)]
struct ImageOption {
    id: i32,
    title: String,
}

#[derive(serde::Serialize)]
struct GalleryImageView {
    id: i32,
    title: String,
}

pub async fn list(State(state): State<AppState>) -> impl IntoResponse {
    let galleries = gallery::Entity::find()
        .order_by_desc(gallery::Column::CreatedAt)
        .all(&state.db)
        .await
        .unwrap_or_default();

    let views: Vec<GalleryView> = galleries
        .iter()
        .map(|g| GalleryView {
            id: g.id,
            title: g.title.clone(),
            description: g.description.clone(),
            image_count: g.image_ids.len(),
            created_at: g.created_at.to_string(),
        })
        .collect();

    let menu = build_menu(&state.db, true).await;
    let tmpl = state.tmpl.get_template("admin/galleries.html").unwrap();
    Html(
        tmpl.render(context! { galleries => views, menu, logged_in => true })
            .unwrap(),
    )
}

pub async fn new_form(State(state): State<AppState>) -> impl IntoResponse {
    let menu = build_menu(&state.db, true).await;
    let tmpl = state.tmpl.get_template("admin/gallery_form.html").unwrap();
    Html(
        tmpl.render(context! { menu, logged_in => true, gallery => () })
            .unwrap(),
    )
}

#[derive(serde::Deserialize)]
pub struct GalleryForm {
    pub title: String,
    pub description: Option<String>,
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    Form(form): Form<GalleryForm>,
) -> impl IntoResponse {
    let saved = gallery::ActiveModel {
        title: Set(form.title),
        description: Set(form.description.filter(|s| !s.is_empty())),
        image_ids: Set(vec![]),
        created_by: Set(user_id),
        ..Default::default()
    }
    .insert(&state.db)
    .await
    .unwrap();
    Redirect::to(&format!("/admin/galerie/{}/edit", saved.id))
}

pub async fn edit_form(State(state): State<AppState>, Path(id): Path<i32>) -> impl IntoResponse {
    let gal = gallery::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();

    let mut gallery_images = Vec::new();
    for img_id in &gal.image_ids {
        if let Ok(Some(img)) = image_entity::Entity::find_by_id(*img_id).one(&state.db).await {
            gallery_images.push(GalleryImageView {
                id: img.id,
                title: img.title.clone(),
            });
        }
    }

    let all_images: Vec<ImageOption> = image_entity::Entity::find()
        .order_by_desc(image_entity::Column::CreatedAt)
        .all(&state.db)
        .await
        .unwrap_or_default()
        .iter()
        .map(|i| ImageOption {
            id: i.id,
            title: i.title.clone(),
        })
        .collect();

    let view = GalleryView {
        id: gal.id,
        title: gal.title.clone(),
        description: gal.description.clone(),
        image_count: gallery_images.len(),
        created_at: gal.created_at.to_string(),
    };

    let menu = build_menu(&state.db, true).await;
    let tmpl = state.tmpl.get_template("admin/gallery_form.html").unwrap();
    Html(
        tmpl.render(context! {
            gallery => view,
            gallery_images,
            all_images,
            menu,
            logged_in => true
        })
        .unwrap(),
    )
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Form(form): Form<GalleryForm>,
) -> impl IntoResponse {
    let model = gallery::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    let mut active: gallery::ActiveModel = model.into();
    active.title = Set(form.title);
    active.description = Set(form.description.filter(|s| !s.is_empty()));
    active.update(&state.db).await.unwrap();
    Redirect::to(&format!("/admin/galerie/{id}/edit"))
}

#[derive(serde::Deserialize)]
pub struct AddImageForm {
    pub image_id: i32,
}

pub async fn add_image(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Form(form): Form<AddImageForm>,
) -> impl IntoResponse {
    let model = gallery::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    let mut ids = model.image_ids.clone();
    ids.push(form.image_id);
    let mut active: gallery::ActiveModel = model.into();
    active.image_ids = Set(ids);
    active.update(&state.db).await.unwrap();
    Redirect::to(&format!("/admin/galerie/{id}/edit"))
}

pub async fn remove_image(
    State(state): State<AppState>,
    Path((gallery_id, image_id)): Path<(i32, i32)>,
) -> impl IntoResponse {
    let model = gallery::Entity::find_by_id(gallery_id)
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    let ids: Vec<i32> = model
        .image_ids
        .iter()
        .copied()
        .filter(|&i| i != image_id)
        .collect();
    let mut active: gallery::ActiveModel = model.into();
    active.image_ids = Set(ids);
    active.update(&state.db).await.unwrap();
    Redirect::to(&format!("/admin/galerie/{gallery_id}/edit"))
}

pub async fn upload_images(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    Path(id): Path<i32>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut files: Vec<Vec<u8>> = Vec::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "files" {
            if let Ok(bytes) = field.bytes().await {
                if !bytes.is_empty() {
                    files.push(bytes.to_vec());
                }
            }
        }
    }

    let model = gallery::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    let mut ids = model.image_ids.clone();

    for (i, file_bytes) in files.iter().enumerate() {
        match process_image(file_bytes) {
            Ok((jpeg_data, thumb_data)) => {
                let saved = image_entity::ActiveModel {
                    title: Set(format!("Gallery {} – {}", id, i + 1)),
                    description: Set(None),
                    data: Set(jpeg_data),
                    thumbnail: Set(thumb_data),
                    created_by: Set(user_id),
                    ..Default::default()
                }
                .insert(&state.db)
                .await
                .unwrap();
                ids.push(saved.id);
            }
            Err(e) => tracing::error!("Image processing error: {e}"),
        }
    }

    let mut active: gallery::ActiveModel = model.into();
    active.image_ids = Set(ids);
    active.update(&state.db).await.unwrap();

    Redirect::to(&format!("/admin/galerie/{id}/edit"))
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<i32>) -> impl IntoResponse {
    let model = gallery::Entity::find_by_id(id)
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    model.delete(&state.db).await.unwrap();
    Redirect::to("/admin/galerie")
}
