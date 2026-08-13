use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};

use crate::auth::InvoicesUser;
use crate::state::AppState;
use db::vat_rates::VatRate;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list))
}

async fn list(
    State(state): State<AppState>,
    InvoicesUser(_user): InvoicesUser,
) -> Result<Json<Vec<VatRate>>, StatusCode> {
    db::vat_rates::list(&state.pool)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
