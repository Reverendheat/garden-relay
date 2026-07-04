use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Serialize;

use crate::{
    state::AppState,
    storage::{ApprovalRequest, ApprovalStatus},
};

pub async fn list_approvals(
    State(state): State<AppState>,
) -> Result<Json<Vec<ApprovalRequest>>, ApprovalApiError> {
    state
        .storage
        .list_approvals()
        .map(Json)
        .map_err(ApprovalApiError::internal)
}

pub async fn get_approval(
    State(state): State<AppState>,
    Path(approval_id): Path<String>,
) -> Result<Json<ApprovalRequest>, ApprovalApiError> {
    state
        .storage
        .get_approval(&approval_id)
        .map_err(ApprovalApiError::internal)?
        .map(Json)
        .ok_or_else(|| ApprovalApiError::not_found(approval_id))
}

pub async fn approve_approval(
    State(state): State<AppState>,
    Path(approval_id): Path<String>,
) -> Result<Json<ApprovalRequest>, ApprovalApiError> {
    decide_approval(state, approval_id, ApprovalStatus::Approved)
}

pub async fn deny_approval(
    State(state): State<AppState>,
    Path(approval_id): Path<String>,
) -> Result<Json<ApprovalRequest>, ApprovalApiError> {
    decide_approval(state, approval_id, ApprovalStatus::Denied)
}

fn decide_approval(
    state: AppState,
    approval_id: String,
    status: ApprovalStatus,
) -> Result<Json<ApprovalRequest>, ApprovalApiError> {
    state
        .storage
        .decide_approval(&approval_id, status)
        .map_err(ApprovalApiError::internal)?
        .map(Json)
        .ok_or_else(|| ApprovalApiError::not_found(approval_id))
}

#[derive(Debug)]
pub struct ApprovalApiError {
    status: StatusCode,
    error: &'static str,
    message: String,
}

impl ApprovalApiError {
    fn not_found(approval_id: String) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            error: "approval_not_found",
            message: format!("approval request '{approval_id}' was not found"),
        }
    }

    fn internal(error: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: "storage_error",
            message: error.to_string(),
        }
    }
}

impl IntoResponse for ApprovalApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(ApprovalApiErrorBody {
                error: self.error,
                message: self.message,
            }),
        )
            .into_response()
    }
}

#[derive(Debug, Serialize)]
struct ApprovalApiErrorBody {
    error: &'static str,
    message: String,
}
