use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
    pub role: String,
}

pub async fn find_by_username(pool: &PgPool, username: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(
        "SELECT id, username, password_hash, role FROM users WHERE username = $1",
    )
    .bind(username)
    .fetch_optional(pool)
    .await
}

pub async fn create(
    pool: &PgPool,
    username: &str,
    password_hash: &str,
) -> Result<User, sqlx::Error> {
    sqlx::query_as::<_, User>(
        "INSERT INTO users (username, password_hash, role) VALUES ($1, $2, 'administrator')
         RETURNING id, username, password_hash, role",
    )
    .bind(username)
    .bind(password_hash)
    .fetch_one(pool)
    .await
}
