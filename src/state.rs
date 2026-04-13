use minijinja::Environment;
use minijinja::value::Value;
use sea_orm::DatabaseConnection;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub tmpl: Arc<Environment<'static>>,
}

fn timeformat(value: Value, format: Option<String>) -> Result<String, minijinja::Error> {
    let s = value.to_string();
    let fmt = format.as_deref().unwrap_or("%d. %m. %Y %H:%M");
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S%.f") {
        return Ok(dt.format(fmt).to_string());
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Ok(dt.format(fmt).to_string());
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
        return Ok(d.format(fmt).to_string());
    }
    Ok(s)
}

pub async fn create_state() -> AppState {
    dotenvy::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db = sea_orm::Database::connect(&database_url)
        .await
        .expect("Failed to connect to database");

    let mut env = Environment::new();
    env.set_loader(minijinja::path_loader("templates"));
    env.add_filter("timeformat", timeformat);

    AppState {
        db,
        tmpl: Arc::new(env),
    }
}
