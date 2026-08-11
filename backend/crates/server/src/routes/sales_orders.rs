use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;
use db::sales_orders::{CreateSalesOrderInput, SalesOrder, SalesOrderWithItems};

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list).post(create)).route("/{id}", get(get_one))
}

async fn list(State(state): State<AppState>, AuthUser(_user): AuthUser) -> Result<Json<Vec<SalesOrder>>, StatusCode> {
    db::sales_orders::list(&state.pool).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn get_one(State(state): State<AppState>, AuthUser(_user): AuthUser, Path(id): Path<Uuid>) -> Result<Json<SalesOrderWithItems>, StatusCode> {
    db::sales_orders::get(&state.pool, id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.map(Json).ok_or(StatusCode::NOT_FOUND)
}

async fn create(State(state): State<AppState>, AuthUser(_user): AuthUser, Json(input): Json<CreateSalesOrderInput>) -> Result<Json<SalesOrderWithItems>, StatusCode> {
    if input.items.is_empty() || !matches!(input.source.as_str(), "manual" | "woocommerce" | "amazon" | "ebay") {
        return Err(StatusCode::BAD_REQUEST);
    }
    db::sales_orders::create(&state.pool, &input).await.map(Json).map_err(|_| StatusCode::BAD_REQUEST)
}
