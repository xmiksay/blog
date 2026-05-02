pub struct Config {
    pub database_url: String,
    pub namespace: String,
}

impl Config {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();
        Self {
            database_url: std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
            namespace: std::env::var("NAMESPACE").unwrap_or_else(|_| "common".into()),
        }
    }
}
