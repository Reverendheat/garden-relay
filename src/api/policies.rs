use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Serialize;

use crate::{policy::StaticPolicy, state::AppState};

pub async fn list_policies(State(state): State<AppState>) -> Json<Vec<StaticPolicy>> {
    Json(state.policy_engine.list_policies())
}

pub async fn upsert_policy(
    State(state): State<AppState>,
    Json(policy): Json<StaticPolicy>,
) -> Result<(StatusCode, Json<StaticPolicy>), PolicyApiError> {
    policy.validate().map_err(PolicyApiError::bad_request)?;
    state
        .storage
        .save_policy(&policy)
        .map_err(PolicyApiError::internal)?;
    state
        .policy_engine
        .add_policy(policy.clone())
        .map_err(PolicyApiError::bad_request)?;

    Ok((StatusCode::CREATED, Json(policy)))
}

#[derive(Debug)]
pub struct PolicyApiError {
    status: StatusCode,
    error: &'static str,
    message: String,
}

impl PolicyApiError {
    fn bad_request(error: anyhow::Error) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            error: "invalid_policy",
            message: error.to_string(),
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

impl IntoResponse for PolicyApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(PolicyApiErrorBody {
                error: self.error,
                message: self.message,
            }),
        )
            .into_response()
    }
}

#[derive(Debug, Serialize)]
struct PolicyApiErrorBody {
    error: &'static str,
    message: String,
}

#[cfg(test)]
mod tests {
    use axum::extract::State;

    use crate::{
        policy::{
            PolicyEngine, PolicyPhase, StaticAction, StaticCondition, StaticEffect,
            StaticEffectKind,
        },
        provider::openai_compatible::OpenAiCompatibleClient,
        state::{AppState, LifecycleStore},
    };

    use super::*;

    #[tokio::test]
    async fn upsert_policy_adds_policy_to_running_state() {
        let state = AppState::new(
            OpenAiCompatibleClient::new("http://127.0.0.1:1".to_owned()),
            LifecycleStore::default(),
            PolicyEngine::default(),
        );
        let policy = tenant_policy();

        let (_, Json(created)) = upsert_policy(State(state.clone()), Json(policy))
            .await
            .expect("policy created");
        let Json(policies) = list_policies(State(state)).await;

        assert_eq!(created.name, "require_tenant");
        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].name, "require_tenant");
    }

    #[tokio::test]
    async fn rejects_invalid_policy() {
        let state = AppState::new(
            OpenAiCompatibleClient::new("http://127.0.0.1:1".to_owned()),
            LifecycleStore::default(),
            PolicyEngine::default(),
        );
        let policy = StaticPolicy {
            name: String::new(),
            ..tenant_policy()
        };

        let error = upsert_policy(State(state), Json(policy))
            .await
            .expect_err("invalid policy");

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
    }

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
}
