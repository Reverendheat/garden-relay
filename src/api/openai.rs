use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header::AUTHORIZATION},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    domain::{
        Message, MessageContent, MessageRole, RelayOperation, RelayOptions, RelayRequest,
        RequestMetadata,
    },
    lifecycle::{LifecyclePhase, RequestLifecycle},
    policy::{PolicyContext, PolicyPhase},
    provider::openai_compatible::ProviderError,
    state::AppState,
};

const GARDEN_REQUEST_ID_HEADER: &str = "x-garden-request-id";

pub async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<(StatusCode, HeaderMap, Json<Value>), ApiError> {
    let mut lifecycle = RequestLifecycle::new();
    lifecycle.record_phase(LifecyclePhase::RequestReceived);

    let request = ChatCompletionRequest::from_body(&body)
        .map_err(|error| fail_lifecycle(&state, &mut lifecycle, error))?;

    lifecycle.record_phase(LifecyclePhase::BeforeInput);

    if request.model.trim().is_empty() {
        return Err(fail_lifecycle(
            &state,
            &mut lifecycle,
            ApiError::invalid_request("model must not be empty"),
        ));
    }

    if request.messages.is_empty() {
        return Err(fail_lifecycle(
            &state,
            &mut lifecycle,
            ApiError::invalid_request("messages must contain at least one item"),
        ));
    }

    if request.stream.unwrap_or(false) {
        return Err(fail_lifecycle(
            &state,
            &mut lifecycle,
            ApiError::unsupported("streaming chat completions are not implemented yet"),
        ));
    }

    let relay_request = request
        .clone()
        .into_relay_request(RequestMetadata::from_headers(&headers));
    lifecycle.set_relay_request(relay_request.clone());

    let request_id = lifecycle.request_id().to_owned();
    let span = tracing::info_span!(
        "garden.request",
        garden.request_id = %request_id,
        llm.model = %relay_request.model,
        garden.tenant_id = relay_request.metadata.tenant_id.as_deref().unwrap_or(""),
        garden.app_id = relay_request.metadata.app_id.as_deref().unwrap_or(""),
        garden.user_id = relay_request.metadata.user_id.as_deref().unwrap_or(""),
    );
    let _span_guard = span.enter();

    tracing::info!(
        model = %relay_request.model,
        tenant_id = relay_request.metadata.tenant_id.as_deref().unwrap_or(""),
        app_id = relay_request.metadata.app_id.as_deref().unwrap_or(""),
        user_id = relay_request.metadata.user_id.as_deref().unwrap_or(""),
        "forwarding OpenAI-compatible chat completion"
    );

    apply_policy_phase(
        &state,
        &mut lifecycle,
        PolicyPhase::BeforeInput,
        &headers,
        &relay_request,
        &body,
        None,
    )
    .map_err(|error| fail_lifecycle(&state, &mut lifecycle, error))?;

    lifecycle.record_phase(LifecyclePhase::BeforeModel);

    apply_policy_phase(
        &state,
        &mut lifecycle,
        PolicyPhase::BeforeModel,
        &headers,
        &relay_request,
        &body,
        None,
    )
    .map_err(|error| fail_lifecycle(&state, &mut lifecycle, error))?;

    let authorization = authorization_from_headers(&headers)
        .map_err(|error| fail_lifecycle(&state, &mut lifecycle, error))?;

    lifecycle.record_phase(LifecyclePhase::ProviderCall);
    lifecycle.record_event(
        "provider_call_started",
        serde_json::json!({ "provider": "openai_compatible" }),
    );
    let provider_response = state
        .openai
        .chat_completions(authorization, &body)
        .await
        .map_err(|error| fail_lifecycle(&state, &mut lifecycle, ApiError::provider(error)))?;
    lifecycle.record_event(
        "provider_call_completed",
        serde_json::json!({ "status_code": provider_response.status.as_u16() }),
    );

    lifecycle.record_phase(LifecyclePhase::AfterModel);
    apply_policy_phase(
        &state,
        &mut lifecycle,
        PolicyPhase::AfterModel,
        &headers,
        &relay_request,
        &body,
        Some(&provider_response.body),
    )
    .map_err(|error| fail_lifecycle(&state, &mut lifecycle, error))?;

    lifecycle.record_phase(LifecyclePhase::BeforeResponse);
    apply_policy_phase(
        &state,
        &mut lifecycle,
        PolicyPhase::BeforeResponse,
        &headers,
        &relay_request,
        &body,
        Some(&provider_response.body),
    )
    .map_err(|error| fail_lifecycle(&state, &mut lifecycle, error))?;

    lifecycle.record_success(provider_response.status);
    lifecycle.record_phase(LifecyclePhase::ResponseSent);
    lifecycle.emit_tracing_events();
    state.lifecycle_store.save(lifecycle.snapshot());

    Ok((
        provider_response.status,
        request_id_headers(lifecycle.request_id()),
        Json(provider_response.body),
    ))
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatCompletionMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: Option<bool>,
    pub metadata: Option<Value>,
}

impl ChatCompletionRequest {
    fn from_body(body: &Value) -> Result<Self, ApiError> {
        serde_json::from_value(body.clone())
            .map_err(|error| ApiError::invalid_request(format!("invalid request body: {error}")))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatCompletionMessage {
    pub role: ChatCompletionRole,
    #[serde(default)]
    pub content: Option<MessageContent>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatCompletionRole {
    System,
    Developer,
    User,
    Assistant,
    Tool,
}

impl From<ChatCompletionRequest> for RelayRequest {
    fn from(request: ChatCompletionRequest) -> Self {
        request.into_relay_request(RequestMetadata::default())
    }
}

impl ChatCompletionRequest {
    fn into_relay_request(self, mut metadata: RequestMetadata) -> RelayRequest {
        metadata.provider_metadata = self.metadata;

        RelayRequest {
            operation: RelayOperation::ChatCompletion,
            model: self.model,
            messages: self
                .messages
                .into_iter()
                .map(|message| Message {
                    role: message.role.into(),
                    content: message.content.unwrap_or(MessageContent::Empty),
                })
                .collect(),
            options: RelayOptions {
                max_tokens: self.max_tokens,
                temperature: self.temperature,
            },
            metadata,
        }
    }
}

impl RequestMetadata {
    fn from_headers(headers: &HeaderMap) -> Self {
        Self {
            tenant_id: header_to_string(headers, "x-garden-tenant"),
            app_id: header_to_string(headers, "x-garden-app"),
            user_id: header_to_string(headers, "x-garden-user"),
            provider_metadata: None,
        }
    }
}

impl From<ChatCompletionRole> for MessageRole {
    fn from(role: ChatCompletionRole) -> Self {
        match role {
            ChatCompletionRole::System => Self::System,
            ChatCompletionRole::Developer => Self::Developer,
            ChatCompletionRole::User => Self::User,
            ChatCompletionRole::Assistant => Self::Assistant,
            ChatCompletionRole::Tool => Self::Tool,
        }
    }
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
    request_id: Option<String>,
}

impl ApiError {
    fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request_error",
            message: message.into(),
            request_id: None,
        }
    }

    fn unsupported(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_IMPLEMENTED,
            code: "unsupported_operation",
            message: message.into(),
            request_id: None,
        }
    }

    fn authentication(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "authentication_error",
            message: message.into(),
            request_id: None,
        }
    }

    fn provider(error: ProviderError) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "provider_error",
            message: error.message,
            request_id: None,
        }
    }

    fn policy_denied(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "policy_denied",
            message: message.into(),
            request_id: None,
        }
    }

    fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(OpenAiErrorResponse {
            error: OpenAiError {
                message: self.message,
                r#type: self.code,
                code: self.code,
            },
        });

        let headers = self
            .request_id
            .as_deref()
            .map(request_id_headers)
            .unwrap_or_default();

        (self.status, headers, body).into_response()
    }
}

#[derive(Debug, Serialize)]
struct OpenAiErrorResponse {
    error: OpenAiError,
}

#[derive(Debug, Serialize)]
struct OpenAiError {
    message: String,
    #[serde(rename = "type")]
    r#type: &'static str,
    code: &'static str,
}

fn header_to_string(headers: &HeaderMap, name: &'static str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}

fn authorization_from_headers(headers: &HeaderMap) -> Result<HeaderValue, ApiError> {
    headers.get(AUTHORIZATION).cloned().ok_or_else(|| {
        ApiError::authentication(
            "Authorization header is required; Garden Relay forwards provider keys and does not store them",
        )
    })
}

fn apply_policy_phase(
    state: &AppState,
    lifecycle: &mut RequestLifecycle,
    phase: PolicyPhase,
    headers: &HeaderMap,
    request: &RelayRequest,
    request_body: &Value,
    response_body: Option<&Value>,
) -> Result<(), ApiError> {
    let ctx = PolicyContext {
        headers,
        request,
        request_body,
        response_body,
    };

    for decision in state.policy_engine.evaluate(phase, &ctx) {
        lifecycle.record_event(
            "policy.evaluated",
            serde_json::json!({
                "policy": decision.policy_name,
                "phase": decision.phase,
                "matched": decision.matched,
            }),
        );

        if let Some(reason) = decision.deny_reason() {
            lifecycle.record_event(
                "policy.effect.applied",
                serde_json::json!({
                    "policy": decision.policy_name,
                    "effect": "deny",
                    "reason": reason,
                }),
            );
            return Err(ApiError::policy_denied(reason));
        }
    }

    Ok(())
}

fn fail_lifecycle(state: &AppState, lifecycle: &mut RequestLifecycle, error: ApiError) -> ApiError {
    lifecycle.record_failure(error.status, error.code);
    lifecycle.emit_tracing_events();
    state.lifecycle_store.save(lifecycle.snapshot());
    error.with_request_id(lifecycle.request_id().to_owned())
}

fn request_id_headers(request_id: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(request_id) {
        headers.insert(GARDEN_REQUEST_ID_HEADER, value);
    }
    headers
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderMap;
    use serde_json::json;

    use super::*;
    use crate::policy::{
        PolicyEngine, StaticAction, StaticCondition, StaticEffectKind, StaticPolicy,
    };
    use crate::provider::openai_compatible::OpenAiCompatibleClient;

    #[tokio::test]
    async fn rejects_missing_authorization_without_storing_provider_keys() {
        let state = test_app_state();
        let request = json!({
            "model": "gpt-4.1-mini",
            "messages": [{ "role": "user", "content": "hello relay" }]
        });

        let error = chat_completions(State(state.clone()), HeaderMap::new(), Json(request))
            .await
            .expect_err("missing auth should be rejected");

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        assert_eq!(error.code, "authentication_error");
        let request_id = error.request_id.expect("request id");
        let snapshot = state
            .lifecycle_store
            .get(&request_id)
            .expect("stored lifecycle snapshot");

        assert_eq!(snapshot.request_id, request_id);
        assert!(matches!(
            snapshot.outcome,
            Some(crate::lifecycle::LifecycleOutcome::Failed {
                status_code: 401,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn rejects_streaming_until_it_is_implemented() {
        let request = json!({
            "model": "gpt-4.1-mini",
            "messages": [{ "role": "user", "content": "hello relay" }],
            "stream": true
        });

        let error = chat_completions(test_state(), HeaderMap::new(), Json(request))
            .await
            .expect_err("streaming should be rejected");

        assert_eq!(error.status, StatusCode::NOT_IMPLEMENTED);
        assert_eq!(error.code, "unsupported_operation");
        assert!(error.request_id.is_some());
    }

    #[tokio::test]
    async fn applies_policy_denies_before_provider_forwarding() {
        let policy_engine = PolicyEngine::from_policies(vec![StaticPolicy {
            name: "require_tenant".to_owned(),
            phase: PolicyPhase::BeforeModel,
            condition: StaticCondition {
                missing_header: Some("x-garden-tenant".to_owned()),
                ..StaticCondition::default()
            },
            action: StaticAction {
                effect: StaticEffectKind::Deny,
                reason: Some("Tenant header required.".to_owned()),
            },
        }]);
        let state = AppState::new(
            OpenAiCompatibleClient::new("http://127.0.0.1:1".to_owned()),
            Default::default(),
            policy_engine,
        );
        let request = json!({
            "model": "gpt-4.1-mini",
            "messages": [{ "role": "user", "content": "hello relay" }]
        });

        let error = chat_completions(State(state.clone()), HeaderMap::new(), Json(request))
            .await
            .expect_err("policy should deny request");

        assert_eq!(error.status, StatusCode::FORBIDDEN);
        assert_eq!(error.code, "policy_denied");
        let request_id = error.request_id.expect("request id");
        let snapshot = state
            .lifecycle_store
            .get(&request_id)
            .expect("stored lifecycle snapshot");

        assert!(
            snapshot
                .events
                .iter()
                .any(|event| event.name == "policy.effect.applied")
        );
    }

    #[test]
    fn garden_headers_become_request_metadata() {
        let mut headers = HeaderMap::new();
        headers.insert("x-garden-tenant", "tenant_123".parse().unwrap());
        headers.insert("x-garden-app", "support_bot".parse().unwrap());
        headers.insert("x-garden-user", "user_456".parse().unwrap());

        let metadata = RequestMetadata::from_headers(&headers);

        assert_eq!(metadata.tenant_id.as_deref(), Some("tenant_123"));
        assert_eq!(metadata.app_id.as_deref(), Some("support_bot"));
        assert_eq!(metadata.user_id.as_deref(), Some("user_456"));
    }

    #[test]
    fn authorization_header_is_passed_through() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, "Bearer sk-test".parse().unwrap());

        let authorization = authorization_from_headers(&headers).expect("authorization header");

        assert_eq!(authorization, "Bearer sk-test");
    }

    #[test]
    fn request_body_preserves_openai_params_for_forwarding() {
        let body = json!({
            "model": "gpt-4.1-mini",
            "messages": [{ "role": "user", "content": "hello relay" }],
            "response_format": { "type": "json_object" }
        });

        let request = ChatCompletionRequest::from_body(&body).expect("valid chat request");
        let relay_request = request.into_relay_request(RequestMetadata::default());

        assert_eq!(relay_request.model, "gpt-4.1-mini");
        assert_eq!(body["response_format"]["type"], "json_object");
    }

    #[test]
    fn request_id_header_is_returned_with_lifecycle_errors() {
        let response = ApiError::invalid_request("bad request")
            .with_request_id("request-123")
            .into_response();

        assert_eq!(
            response.headers().get(GARDEN_REQUEST_ID_HEADER).unwrap(),
            "request-123"
        );
    }

    fn test_state() -> State<AppState> {
        State(test_app_state())
    }

    fn test_app_state() -> AppState {
        AppState::new(
            OpenAiCompatibleClient::new("http://127.0.0.1:1".to_owned()),
            Default::default(),
            PolicyEngine::default(),
        )
    }
}
