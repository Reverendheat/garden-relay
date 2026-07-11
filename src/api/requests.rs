use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Serialize;

use crate::{lifecycle::LifecycleSnapshot, state::AppState};

#[derive(Debug, serde::Deserialize)]
pub struct RequestFilters {
    pub tenant_id: Option<String>,
}

pub async fn list_requests(
    State(state): State<AppState>,
    Query(filters): Query<RequestFilters>,
) -> Json<Vec<LifecycleSnapshot>> {
    let persisted = match filters.tenant_id.as_deref() {
        Some(tenant_id) => state.storage.list_lifecycles_for_tenant(tenant_id),
        None => state.storage.list_lifecycles(),
    };
    match persisted {
        Ok(snapshots) => Json(snapshots),
        Err(error) => {
            tracing::error!("failed to list persisted request lifecycles: {error}");
            Json(state.lifecycle_store.list())
        }
    }
}

pub async fn request_timeline(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
    Query(filters): Query<RequestFilters>,
) -> Result<Json<LifecycleSnapshot>, RequestLookupError> {
    if let Some(snapshot) = state.lifecycle_store.get(&request_id)
        && filters.tenant_id.as_deref().is_none_or(|tenant_id| {
            snapshot
                .relay_request
                .as_ref()
                .and_then(|request| request.metadata.tenant_id.as_deref())
                == Some(tenant_id)
        })
    {
        return Ok(Json(snapshot));
    }

    let persisted = match filters.tenant_id.as_deref() {
        Some(tenant_id) => state
            .storage
            .get_lifecycle_for_tenant(tenant_id, &request_id),
        None => state.storage.get_lifecycle(&request_id),
    };
    match persisted {
        Ok(Some(snapshot)) => Ok(Json(snapshot)),
        Ok(None) => Err(RequestLookupError { request_id }),
        Err(error) => {
            tracing::error!(%request_id, "failed to load persisted request lifecycle: {error}");
            Err(RequestLookupError { request_id })
        }
    }
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
