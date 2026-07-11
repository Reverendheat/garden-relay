use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header::SET_COOKIE},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::{CreatedApiKey, CreatedOperator, OperatorContext},
    identity::{ApiKeySummary, App, Operator, Tenant, User},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub operator: Operator,
    pub expires_at: Option<i64>,
}

pub async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<SessionResponse>), AdminError> {
    let session = state
        .auth
        .login_operator(&input.token)
        .map_err(|_| AdminError::unauthorized())?;
    let mut headers = HeaderMap::new();
    let secure = if state.session_cookie_secure {
        "; Secure"
    } else {
        ""
    };
    let cookie = format!(
        "garden_session={}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}{}",
        session.secret,
        (session.expires_at - unix_timestamp()).max(0),
        secure,
    );
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(AdminError::internal)?,
    );
    Ok((
        headers,
        Json(SessionResponse {
            operator: session.context.operator,
            expires_at: Some(session.expires_at),
        }),
    ))
}

pub async fn logout(
    State(state): State<AppState>,
    Extension(context): Extension<OperatorContext>,
) -> Result<(HeaderMap, StatusCode), AdminError> {
    state
        .auth
        .logout_operator(&context.session_id)
        .map_err(AdminError::internal)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_static("garden_session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0"),
    );
    Ok((headers, StatusCode::NO_CONTENT))
}

pub async fn session(Extension(context): Extension<OperatorContext>) -> Json<SessionResponse> {
    Json(SessionResponse {
        operator: context.operator,
        expires_at: Some(context.expires_at),
    })
}

#[derive(Debug, Deserialize)]
pub struct NameRequest {
    pub name: String,
}

pub async fn list_tenants(State(state): State<AppState>) -> Result<Json<Vec<Tenant>>, AdminError> {
    Ok(Json(
        state.storage.list_tenants().map_err(AdminError::internal)?,
    ))
}

pub async fn create_tenant(
    State(state): State<AppState>,
    Json(input): Json<NameRequest>,
) -> Result<(StatusCode, Json<Tenant>), AdminError> {
    let tenant = state
        .storage
        .create_tenant(&input.name)
        .map_err(AdminError::bad_request)?;
    Ok((StatusCode::CREATED, Json(tenant)))
}

#[derive(Debug, Deserialize)]
pub struct UpdateTenantRequest {
    pub name: Option<String>,
    pub active: Option<bool>,
}

pub async fn update_tenant(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    Json(input): Json<UpdateTenantRequest>,
) -> Result<Json<Tenant>, AdminError> {
    state
        .storage
        .update_tenant(&tenant_id, input.name.as_deref(), input.active)
        .map_err(AdminError::bad_request)?
        .map(Json)
        .ok_or_else(|| AdminError::not_found("Tenant not found"))
}

pub async fn list_apps(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
) -> Result<Json<Vec<App>>, AdminError> {
    Ok(Json(
        state
            .storage
            .list_apps(&tenant_id)
            .map_err(AdminError::internal)?,
    ))
}

pub async fn create_app(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    Json(input): Json<NameRequest>,
) -> Result<(StatusCode, Json<App>), AdminError> {
    let app = state
        .storage
        .create_app(&tenant_id, &input.name)
        .map_err(AdminError::bad_request)?;
    Ok((StatusCode::CREATED, Json(app)))
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub external_id: String,
    pub display_name: Option<String>,
}

pub async fn list_users(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
) -> Result<Json<Vec<User>>, AdminError> {
    Ok(Json(
        state
            .storage
            .list_users(&tenant_id)
            .map_err(AdminError::internal)?,
    ))
}

pub async fn create_user(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    Json(input): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<User>), AdminError> {
    let user = state
        .storage
        .create_user(
            &tenant_id,
            &input.external_id,
            input.display_name.as_deref(),
        )
        .map_err(AdminError::bad_request)?;
    Ok((StatusCode::CREATED, Json(user)))
}

#[derive(Debug, Deserialize)]
pub struct CreateKeyRequest {
    pub tenant_id: String,
    pub name: String,
    pub expires_at: Option<i64>,
}

pub async fn list_keys(
    State(state): State<AppState>,
    Path(app_id): Path<String>,
) -> Result<Json<Vec<ApiKeySummary>>, AdminError> {
    Ok(Json(
        state
            .storage
            .list_api_keys(&app_id)
            .map_err(AdminError::internal)?,
    ))
}

pub async fn create_key(
    State(state): State<AppState>,
    Path(app_id): Path<String>,
    Json(input): Json<CreateKeyRequest>,
) -> Result<(StatusCode, Json<CreatedApiKey>), AdminError> {
    let key = state
        .auth
        .create_api_key(&input.tenant_id, &app_id, &input.name, input.expires_at)
        .map_err(AdminError::bad_request)?;
    Ok((StatusCode::CREATED, Json(key)))
}

pub async fn revoke_key(
    State(state): State<AppState>,
    Path((app_id, key_id)): Path<(String, String)>,
) -> Result<StatusCode, AdminError> {
    if state
        .storage
        .revoke_api_key(&app_id, &key_id)
        .map_err(AdminError::internal)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AdminError::not_found("API key not found"))
    }
}

pub async fn list_operators(
    State(state): State<AppState>,
) -> Result<Json<Vec<Operator>>, AdminError> {
    Ok(Json(
        state
            .storage
            .list_operators()
            .map_err(AdminError::internal)?,
    ))
}

pub async fn create_operator(
    State(state): State<AppState>,
    Json(input): Json<NameRequest>,
) -> Result<(StatusCode, Json<CreatedOperator>), AdminError> {
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .auth
                .create_operator(&input.name)
                .map_err(AdminError::bad_request)?,
        ),
    ))
}

pub async fn deactivate_operator(
    State(state): State<AppState>,
    Extension(context): Extension<OperatorContext>,
    Path(operator_id): Path<String>,
) -> Result<StatusCode, AdminError> {
    if context.operator.id == operator_id {
        return Err(AdminError::bad_request(anyhow::anyhow!(
            "operators cannot deactivate their own account"
        )));
    }
    if state
        .storage
        .deactivate_operator(&operator_id)
        .map_err(AdminError::internal)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AdminError::not_found("Operator not found"))
    }
}

#[derive(Debug)]
pub struct AdminError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl AdminError {
    fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "invalid_operator_credentials",
            message: "Valid operator credentials are required.".to_owned(),
        }
    }
    fn bad_request(error: anyhow::Error) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request",
            message: error.to_string(),
        }
    }
    fn internal(error: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: error.to_string(),
        }
    }
    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: message.into(),
        }
    }
}

impl IntoResponse for AdminError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.code, "message": self.message })),
        )
            .into_response()
    }
}

fn unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}
