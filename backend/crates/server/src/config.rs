pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub admin_username: String,
    pub admin_password: String,
    pub pdf_storage_dir: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
            jwt_secret: std::env::var("JWT_SECRET").expect("JWT_SECRET must be set"),
            admin_username: std::env::var("ADMIN_USERNAME").unwrap_or_else(|_| "admin".into()),
            admin_password: std::env::var("ADMIN_PASSWORD").expect("ADMIN_PASSWORD must be set"),
            pdf_storage_dir: std::env::var("PDF_STORAGE_DIR")
                .unwrap_or_else(|_| "/data/invoices".into()),
        }
    }
}
