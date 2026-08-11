use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
}

pub async fn find_by_username(pool: &PgPool, username: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as!(
        User,
        "SELECT id, username, password_hash FROM users WHERE username = $1",
        username
    )
    .fetch_optional(pool)
    .await
}

pub async fn create(
    pool: &PgPool,
    username: &str,
    password_hash: &str,
) -> Result<User, sqlx::Error> {
    sqlx::query_as!(
        User,
        "INSERT INTO users (username, password_hash) VALUES ($1, $2)
         RETURNING id, username, password_hash",
        username,
        password_hash
    )
    .fetch_one(pool)
    .await
}
