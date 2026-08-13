use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use domain::invoice_status::InvoiceStatus;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::{CorrectionsUser, InvoicesUser};
use crate::pdf_gen;
use crate::state::AppState;
use db::invoices::{
    CorrectionInput, Invoice, InvoiceInput, InvoiceListItem, InvoiceWithLineItems, LineItemInput,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(get_one).put(update).delete(delete_one))
        .route("/{id}/status", post(transition))
        .route("/{id}/pdf", get(download_pdf))
        .route("/{id}/corrections", post(create_correction))
        .route("/{id}/line-items", post(add_line_item))
        .route(
            "/{id}/line-items/{line_item_id}",
            axum::routing::put(update_line_item).delete(delete_line_item),
        )
}

async fn create_correction(
    State(state): State<AppState>,
    CorrectionsUser(user): CorrectionsUser,
    headers: axum::http::HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<CorrectionInput>,
) -> Result<(StatusCode, Json<db::invoices::CorrectionCreation>), StatusCode> {
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    db::invoices::create_correction(&state.pool, user.id, id, &input, idempotency_key)
        .await
        .map(|result| {
            (
                if result.duplicate {
                    StatusCode::OK
                } else {
                    StatusCode::CREATED
                },
                Json(result),
            )
        })
        .map_err(|error| match error {
            db::invoices::CorrectionError::NotFound => StatusCode::NOT_FOUND,
            db::invoices::CorrectionError::InvalidInput => StatusCode::BAD_REQUEST,
            db::invoices::CorrectionError::NotIssued
            | db::invoices::CorrectionError::CorrectionOfCorrection
            | db::invoices::CorrectionError::AlreadyCorrected => StatusCode::CONFLICT,
            db::invoices::CorrectionError::Sqlx(_) => StatusCode::INTERNAL_SERVER_ERROR,
        })
}

async fn list(
    State(state): State<AppState>,
    InvoicesUser(_user): InvoicesUser,
) -> Result<Json<Vec<InvoiceListItem>>, StatusCode> {
    db::invoices::list(&state.pool)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn get_one(
    State(state): State<AppState>,
    InvoicesUser(_user): InvoicesUser,
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
    InvoicesUser(_user): InvoicesUser,
    Json(input): Json<InvoiceInput>,
) -> Result<Json<Invoice>, StatusCode> {
    db::invoices::create(&state.pool, &input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn update(
    State(state): State<AppState>,
    InvoicesUser(_user): InvoicesUser,
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
    InvoicesUser(_user): InvoicesUser,
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
    InvoicesUser(_user): InvoicesUser,
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

/// PDFs are generated lazily on first download rather than during the `sent`
/// transition: it decouples the (fast, reliable) status change from PDF
/// rendering, and if rendering ever fails, the next download attempt just
/// tries again instead of leaving the invoice stuck with no recovery path.
async fn download_pdf(
    State(state): State<AppState>,
    InvoicesUser(_user): InvoicesUser,
    Path(id): Path<Uuid>,
) -> Result<Response, StatusCode> {
    let invoice = db::invoices::get_bare(&state.pool, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if invoice.status == "draft" {
        return Err(StatusCode::CONFLICT);
    }

    let existing_path = invoice
        .pdf_path
        .as_deref()
        .filter(|p| std::path::Path::new(p).exists());
    let path = match existing_path {
        Some(p) => p.to_string(),
        None => {
            pdf_gen::generate_and_store(&state, id)
                .await
                .map_err(|err| {
                    tracing::error!(?err, invoice_id = %id, "pdf generation failed");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            db::invoices::get_bare(&state.pool, id)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .and_then(|inv| inv.pdf_path)
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
        }
    };

    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let filename = invoice.invoice_number.unwrap_or_else(|| "rechnung".into());

    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}.pdf\""),
            ),
        ],
        Body::from(bytes),
    )
        .into_response())
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
    InvoicesUser(_user): InvoicesUser,
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
    InvoicesUser(_user): InvoicesUser,
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
    InvoicesUser(_user): InvoicesUser,
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
