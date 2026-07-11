use axum::{Json, Router, middleware, routing::get};
use serde::Serialize;

use crate::state::AppState;

pub mod admin;
pub mod openai;
pub mod playground;
pub mod policies;
pub mod requests;
pub mod ui;

pub fn router(state: AppState) -> Router {
    let relay_routes = Router::new()
        .route(
            "/v1/chat/completions",
            axum::routing::post(openai::chat_completions),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::relay_auth,
        ));

    let admin_routes = Router::new()
        .route("/v1/admin/logout", axum::routing::post(admin::logout))
        .route("/v1/admin/session", get(admin::session))
        .route(
            "/v1/tenants",
            get(admin::list_tenants).post(admin::create_tenant),
        )
        .route(
            "/v1/tenants/{tenant_id}",
            axum::routing::patch(admin::update_tenant),
        )
        .route(
            "/v1/tenants/{tenant_id}/apps",
            get(admin::list_apps).post(admin::create_app),
        )
        .route(
            "/v1/tenants/{tenant_id}/users",
            get(admin::list_users).post(admin::create_user),
        )
        .route(
            "/v1/apps/{app_id}/keys",
            get(admin::list_keys).post(admin::create_key),
        )
        .route(
            "/v1/apps/{app_id}/keys/{key_id}",
            axum::routing::delete(admin::revoke_key),
        )
        .route(
            "/v1/operators",
            get(admin::list_operators).post(admin::create_operator),
        )
        .route(
            "/v1/operators/{operator_id}",
            axum::routing::delete(admin::deactivate_operator),
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
            "/v1/scoped-policies",
            get(policies::list_scoped_policies).post(policies::upsert_scoped_policy),
        )
        .route(
            "/v1/playground/evaluate",
            axum::routing::post(playground::evaluate_policy),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::operator_auth,
        ));

    Router::new()
        .route("/healthz", get(healthz))
        .route("/ui", get(ui::admin_ui))
        .route("/v1/admin/login", axum::routing::post(admin::login))
        .merge(relay_routes)
        .merge(admin_routes)
        .with_state(state)
}

async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse { ok: true })
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    ok: bool,
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode, header::SET_COOKIE},
    };
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use crate::{
        auth::{AuthMode, AuthService},
        policy::PolicyEngine,
        provider::openai_compatible::OpenAiCompatibleClient,
        state::{AppState, LifecycleStore},
        storage::Storage,
    };

    use super::router;

    fn test_state() -> AppState {
        let storage = Storage::in_memory().expect("storage");
        let mut state = AppState::with_storage(
            OpenAiCompatibleClient::new("http://127.0.0.1:1".to_owned()),
            LifecycleStore::default(),
            PolicyEngine::default(),
            storage.clone(),
        );
        state.auth = AuthService::new(storage)
            .with_operator_config(Some("bootstrap-secret".to_owned()), 3600);
        state
    }

    async fn response_json(response: axum::response::Response) -> Value {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        serde_json::from_slice(&bytes).expect("JSON response")
    }

    #[tokio::test]
    async fn admin_routes_require_a_session_and_accept_the_login_cookie() {
        let state = test_state();
        let app = router(state);
        let denied = app
            .clone()
            .oneshot(Request::get("/v1/tenants").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);

        let login = app
            .clone()
            .oneshot(
                Request::post("/v1/admin/login")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"token":"bootstrap-secret"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login.status(), StatusCode::OK);
        let cookie = login
            .headers()
            .get(SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();

        let created = app
            .oneshot(
                Request::post("/v1/tenants")
                    .header("content-type", "application/json")
                    .header("cookie", cookie)
                    .body(Body::from(r#"{"name":"Acme"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);
        assert_eq!(response_json(created).await["name"], "Acme");
    }

    #[tokio::test]
    async fn required_relay_auth_rejects_bad_keys_and_overrides_identity_headers() {
        let state = test_state().with_auth_mode(AuthMode::Required);
        let tenant = state.storage.create_tenant("Acme").unwrap();
        let app_record = state.storage.create_app(&tenant.id, "Support").unwrap();
        let key = state
            .auth
            .create_api_key(&tenant.id, &app_record.id, "Test", None)
            .unwrap();
        let app = router(state.clone());
        let body = json!({
            "model": "gpt-4.1-mini",
            "messages": [{ "role": "user", "content": "hello" }]
        });

        let denied = app
            .clone()
            .oneshot(
                Request::post("/v1/chat/completions")
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer provider-key")
                    .header("x-garden-api-key", "bad-secret-value")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
        let failures = state.storage.list_lifecycles().unwrap();
        assert_eq!(failures.len(), 1);
        assert!(failures[0].relay_request.is_none());
        assert!(
            !serde_json::to_string(&failures[0])
                .unwrap()
                .contains("bad-secret-value")
        );

        let invalid_user = app
            .clone()
            .oneshot(
                Request::post("/v1/chat/completions")
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer provider-key")
                    .header("x-garden-api-key", &key.secret)
                    .header("x-garden-user", "user_from_another_tenant")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invalid_user.status(), StatusCode::UNAUTHORIZED);

        let forwarded = app
            .oneshot(
                Request::post("/v1/chat/completions")
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer provider-key")
                    .header("x-garden-api-key", key.secret)
                    .header("x-garden-tenant", "attacker-selected-tenant")
                    .header("x-garden-app", "attacker-selected-app")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(forwarded.status(), StatusCode::BAD_GATEWAY);
        let lifecycles = state
            .storage
            .list_lifecycles_for_tenant(&tenant.id)
            .unwrap();
        assert_eq!(lifecycles.len(), 1);
        let metadata = &lifecycles[0].relay_request.as_ref().unwrap().metadata;
        assert_eq!(metadata.tenant_id.as_deref(), Some(tenant.id.as_str()));
        assert_eq!(metadata.app_id.as_deref(), Some(app_record.id.as_str()));
    }

    #[tokio::test]
    async fn relay_auth_rollout_modes_have_distinct_enforcement() {
        let body = json!({
            "model": "gpt-4.1-mini",
            "messages": [{ "role": "user", "content": "hello" }]
        });
        for (mode, expected) in [
            (AuthMode::Disabled, StatusCode::BAD_GATEWAY),
            (AuthMode::Optional, StatusCode::BAD_GATEWAY),
            (AuthMode::Required, StatusCode::UNAUTHORIZED),
        ] {
            let response = router(test_state().with_auth_mode(mode))
                .oneshot(
                    Request::post("/v1/chat/completions")
                        .header("content-type", "application/json")
                        .header("authorization", "Bearer provider-key")
                        .body(Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected, "mode: {mode:?}");
        }
    }
}
