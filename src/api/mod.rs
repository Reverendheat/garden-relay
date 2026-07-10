use axum::{Json, Router, routing::get};
use serde::Serialize;

use crate::state::AppState;

pub mod openai;
pub mod playground;
pub mod policies;
pub mod requests;
pub mod ui;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/ui", get(ui::admin_ui))
        .route(
            "/v1/chat/completions",
            axum::routing::post(openai::chat_completions),
        )
        .route("/v1/requests", get(requests::list_requests))
        .route(
            "/v1/requests/{request_id}/timeline",
            get(requests::request_timeline),
        )
        .route(
            "/v1/policies",
            get(policies::list_policies).post(policies::upsert_policy),
        )
        .route(
            "/v1/playground/evaluate",
            axum::routing::post(playground::evaluate_policy),
        )
        .with_state(state)
}

async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse { ok: true })
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    ok: bool,
}
