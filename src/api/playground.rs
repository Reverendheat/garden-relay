use std::collections::BTreeMap;

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    api::openai::ChatCompletionRequest,
    domain::RequestMetadata,
    policy::{PolicyContext, PolicyDecision, PolicyEngine, StaticPolicy},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct PlaygroundEvaluationRequest {
    pub policy: StaticPolicy,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    pub request: Value,
    #[serde(default)]
    pub response: Option<Value>,
    #[serde(default)]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub app_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PlaygroundEvaluationResponse {
    pub matched: bool,
    pub decision: PolicyDecision,
}

pub async fn evaluate_policy(
    State(state): State<AppState>,
    Json(input): Json<PlaygroundEvaluationRequest>,
) -> Result<Json<PlaygroundEvaluationResponse>, PlaygroundError> {
    input.policy.validate().map_err(PlaygroundError::policy)?;
    if input.app_id.is_some() && input.tenant_id.is_none() {
        return Err(PlaygroundError::request("app scope requires a tenant"));
    }
    if let Some(tenant_id) = &input.tenant_id
        && !state
            .storage
            .scope_exists(tenant_id, input.app_id.as_deref())
            .map_err(|error| PlaygroundError::internal(error.to_string()))?
    {
        return Err(PlaygroundError::request("selected scope does not exist"));
    }
    let mut headers = build_headers(input.headers)?;
    if let Some(tenant_id) = &input.tenant_id {
        headers.remove("x-garden-tenant");
        headers.remove("x-garden-app");
        headers.insert(
            "x-garden-tenant",
            HeaderValue::from_str(tenant_id)
                .map_err(|error| PlaygroundError::headers(error.to_string()))?,
        );
        if let Some(app_id) = &input.app_id {
            headers.insert(
                "x-garden-app",
                HeaderValue::from_str(app_id)
                    .map_err(|error| PlaygroundError::headers(error.to_string()))?,
            );
        }
    }
    let chat_request = ChatCompletionRequest::from_body(&input.request)
        .map_err(|error| PlaygroundError::request(error.message()))?;

    if chat_request.model.trim().is_empty() {
        return Err(PlaygroundError::request("model must not be empty"));
    }
    if chat_request.messages.is_empty() {
        return Err(PlaygroundError::request(
            "messages must contain at least one item",
        ));
    }

    let relay_request = chat_request.into_relay_request(RequestMetadata::from_headers(&headers));
    let phase = input.policy.phase;
    let engine = PolicyEngine::from_policies(vec![input.policy]);
    let decisions = engine.evaluate(
        phase,
        &PolicyContext {
            headers: &headers,
            request: &relay_request,
            request_body: &input.request,
            response_body: input.response.as_ref(),
        },
    );
    let decision = decisions
        .into_iter()
        .next()
        .ok_or_else(|| PlaygroundError::internal("simulation produced no decision"))?;

    Ok(Json(PlaygroundEvaluationResponse {
        matched: decision.matched,
        decision,
    }))
}

fn build_headers(headers: BTreeMap<String, String>) -> Result<HeaderMap, PlaygroundError> {
    let mut result = HeaderMap::new();
    for (name, value) in headers {
        let name = HeaderName::try_from(name.as_str())
            .map_err(|error| PlaygroundError::headers(format!("invalid header name: {error}")))?;
        let value = HeaderValue::try_from(value.as_str()).map_err(|error| {
            PlaygroundError::headers(format!("invalid value for header '{name}': {error}"))
        })?;
        result.insert(name, value);
    }
    Ok(result)
}

#[derive(Debug)]
pub struct PlaygroundError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl PlaygroundError {
    fn policy(error: anyhow::Error) -> Self {
        Self::bad_request("invalid_policy", error.to_string())
    }

    fn request(message: impl Into<String>) -> Self {
        Self::bad_request("invalid_request", message)
    }

    fn headers(message: impl Into<String>) -> Self {
        Self::bad_request("invalid_headers", message)
    }

    fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "simulation_error",
            message: message.into(),
        }
    }
}

impl IntoResponse for PlaygroundError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({
                "error": self.code,
                "message": self.message,
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::policy::{
        PolicyPhase, StaticAction, StaticCondition, StaticEffect, StaticEffectKind,
    };
    use crate::{provider::openai_compatible::OpenAiCompatibleClient, state::LifecycleStore};

    use super::*;

    fn tenant_policy() -> StaticPolicy {
        StaticPolicy {
            name: "require_tenant".to_owned(),
            phase: PolicyPhase::BeforeModel,
            condition: StaticCondition {
                missing_header: Some("x-garden-tenant".to_owned()),
                ..StaticCondition::default()
            },
            action: StaticAction::Single(StaticEffect {
                effect: StaticEffectKind::Deny,
                reason: Some("Tenant header is required.".to_owned()),
                level: None,
                message: None,
                tools: None,
                mode: None,
                messages: None,
            }),
        }
    }

    fn sample_request() -> Value {
        json!({
            "model": "gpt-4.1-mini",
            "messages": [{ "role": "user", "content": "hello" }]
        })
    }

    fn test_state() -> AppState {
        AppState::new(
            OpenAiCompatibleClient::new("http://127.0.0.1:1".to_owned()),
            LifecycleStore::default(),
            PolicyEngine::default(),
        )
    }

    #[tokio::test]
    async fn evaluates_a_draft_without_persisting_it() {
        let Json(result) = evaluate_policy(
            State(test_state()),
            Json(PlaygroundEvaluationRequest {
                policy: tenant_policy(),
                headers: BTreeMap::new(),
                request: sample_request(),
                response: None,
                tenant_id: None,
                app_id: None,
            }),
        )
        .await
        .expect("evaluation succeeds");

        assert!(result.matched);
        assert_eq!(result.decision.policy_name, "require_tenant");
        assert_eq!(result.decision.effects.len(), 1);
    }

    #[tokio::test]
    async fn sample_headers_can_make_the_policy_not_match() {
        let Json(result) = evaluate_policy(
            State(test_state()),
            Json(PlaygroundEvaluationRequest {
                policy: tenant_policy(),
                headers: BTreeMap::from([("x-garden-tenant".to_owned(), "tenant_123".to_owned())]),
                request: sample_request(),
                response: None,
                tenant_id: None,
                app_id: None,
            }),
        )
        .await
        .expect("evaluation succeeds");

        assert!(!result.matched);
        assert!(result.decision.effects.is_empty());
    }

    #[tokio::test]
    async fn rejects_an_invalid_sample_request() {
        let error = evaluate_policy(
            State(test_state()),
            Json(PlaygroundEvaluationRequest {
                policy: tenant_policy(),
                headers: BTreeMap::new(),
                request: json!({ "model": "gpt-4.1-mini", "messages": [] }),
                response: None,
                tenant_id: None,
                app_id: None,
            }),
        )
        .await
        .expect_err("empty messages are invalid");

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "invalid_request");
    }
}
