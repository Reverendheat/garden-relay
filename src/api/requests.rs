use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Serialize;

use crate::{lifecycle::LifecycleSnapshot, state::AppState};

pub async fn list_requests(State(state): State<AppState>) -> Json<Vec<LifecycleSnapshot>> {
    Json(state.lifecycle_store.list())
}

pub async fn request_timeline(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
) -> Result<Json<LifecycleSnapshot>, RequestLookupError> {
    state
        .lifecycle_store
        .get(&request_id)
        .map(Json)
        .ok_or(RequestLookupError { request_id })
}

#[derive(Debug)]
pub struct RequestLookupError {
    request_id: String,
}

impl axum::response::IntoResponse for RequestLookupError {
    fn into_response(self) -> axum::response::Response {
        (
            StatusCode::NOT_FOUND,
            Json(RequestLookupErrorBody {
                error: "request_not_found",
                request_id: self.request_id,
            }),
        )
            .into_response()
    }
}

#[derive(Debug, Serialize)]
struct RequestLookupErrorBody {
    error: &'static str,
    request_id: String,
}
