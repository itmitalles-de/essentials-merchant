use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use uuid::Uuid;

use crate::auth::{CatalogUser, InventoryUser};
use crate::state::AppState;
use db::articles::{Article, ArticleInput};
use db::stock_movements::{ManualAdjustmentInput, StockMovement};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(get_one).put(update).delete(delete_one))
        .route(
            "/{id}/stock-movements",
            get(list_stock_movements).post(create_adjustment),
        )
}

async fn list(
    State(state): State<AppState>,
    CatalogUser(_user): CatalogUser,
) -> Result<Json<Vec<Article>>, StatusCode> {
    db::articles::list(&state.pool)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn get_one(
    State(state): State<AppState>,
    CatalogUser(_user): CatalogUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Article>, StatusCode> {
    db::articles::get(&state.pool, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn create(
    State(state): State<AppState>,
    CatalogUser(_user): CatalogUser,
    Json(input): Json<ArticleInput>,
) -> Result<Json<Article>, StatusCode> {
    db::articles::create(&state.pool, &input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn update(
    State(state): State<AppState>,
    CatalogUser(_user): CatalogUser,
    Path(id): Path<Uuid>,
    Json(input): Json<ArticleInput>,
) -> Result<Json<Article>, StatusCode> {
    db::articles::update(&state.pool, id, &input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn delete_one(
    State(state): State<AppState>,
    CatalogUser(_user): CatalogUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    db::articles::delete(&state.pool, id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|err| match err {
            db::articles::DeleteError::NotFound => StatusCode::NOT_FOUND,
            db::articles::DeleteError::InUse => StatusCode::CONFLICT,
            db::articles::DeleteError::Sqlx(_) => StatusCode::INTERNAL_SERVER_ERROR,
        })
}

async fn list_stock_movements(
    State(state): State<AppState>,
    InventoryUser(_user): InventoryUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<StockMovement>>, StatusCode> {
    if db::articles::get(&state.pool, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .is_none()
    {
        return Err(StatusCode::NOT_FOUND);
    }

    db::stock_movements::list_for_article(&state.pool, id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn create_adjustment(
    State(state): State<AppState>,
    InventoryUser(_user): InventoryUser,
    Path(id): Path<Uuid>,
    Json(input): Json<ManualAdjustmentInput>,
) -> Result<Json<StockMovement>, StatusCode> {
    if db::articles::get(&state.pool, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .is_none()
    {
        return Err(StatusCode::NOT_FOUND);
    }

    db::stock_movements::create_manual(&state.pool, id, &input)
        .await
        .map(Json)
        .map_err(|err| match err {
            db::stock_movements::AdjustmentError::InvalidType => StatusCode::BAD_REQUEST,
            db::stock_movements::AdjustmentError::Sqlx(_) => StatusCode::INTERNAL_SERVER_ERROR,
        })
}
