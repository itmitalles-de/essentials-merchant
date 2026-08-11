use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use domain::invoice_status::InvoiceStatus;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;
use db::invoices::{Invoice, InvoiceInput, InvoiceListItem, InvoiceWithLineItems, LineItemInput};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(get_one).put(update).delete(delete_one))
        .route("/{id}/status", post(transition))
        .route("/{id}/line-items", post(add_line_item))
        .route(
            "/{id}/line-items/{line_item_id}",
            axum::routing::put(update_line_item).delete(delete_line_item),
        )
}

async fn list(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
) -> Result<Json<Vec<InvoiceListItem>>, StatusCode> {
    db::invoices::list(&state.pool)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn get_one(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<InvoiceWithLineItems>, StatusCode> {
    db::invoices::get(&state.pool, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn create(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    Json(input): Json<InvoiceInput>,
) -> Result<Json<Invoice>, StatusCode> {
    db::invoices::create(&state.pool, &input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn update(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<InvoiceInput>,
) -> Result<Json<Invoice>, StatusCode> {
    let existing = db::invoices::get_bare(&state.pool, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if existing.status != "draft" {
        return Err(StatusCode::CONFLICT);
    }
    db::invoices::update(&state.pool, id, &input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn delete_one(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let existing = db::invoices::get_bare(&state.pool, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if existing.status != "draft" {
        return Err(StatusCode::CONFLICT);
    }
    let deleted = db::invoices::delete(&state.pool, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[derive(Deserialize)]
struct StatusTransitionInput {
    status: String,
}

async fn transition(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<StatusTransitionInput>,
) -> Result<Json<Invoice>, StatusCode> {
    let target = InvoiceStatus::parse(&input.status).ok_or(StatusCode::BAD_REQUEST)?;
    db::invoices::transition_status(&state.pool, id, target)
        .await
        .map(Json)
        .map_err(|err| match err {
            db::invoices::TransitionError::NotFound => StatusCode::NOT_FOUND,
            db::invoices::TransitionError::InvalidTransition { .. } => StatusCode::CONFLICT,
            db::invoices::TransitionError::Sqlx(_) => StatusCode::INTERNAL_SERVER_ERROR,
        })
}

async fn require_draft(state: &AppState, invoice_id: Uuid) -> Result<(), StatusCode> {
    let existing = db::invoices::get_bare(&state.pool, invoice_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if existing.status != "draft" {
        return Err(StatusCode::CONFLICT);
    }
    Ok(())
}

async fn add_line_item(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<LineItemInput>,
) -> Result<Json<db::invoices::InvoiceLineItem>, StatusCode> {
    require_draft(&state, id).await?;
    db::invoices::add_line_item(&state.pool, id, &input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn update_line_item(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    Path((id, line_item_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<LineItemInput>,
) -> Result<Json<db::invoices::InvoiceLineItem>, StatusCode> {
    require_draft(&state, id).await?;
    db::invoices::update_line_item(&state.pool, id, line_item_id, &input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn delete_line_item(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    Path((id, line_item_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    require_draft(&state, id).await?;
    let deleted = db::invoices::delete_line_item(&state.pool, id, line_item_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
