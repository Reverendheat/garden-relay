use std::{
    collections::{HashMap, VecDeque},
    str::FromStr,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::{
    identity::{
        ApiKeyRecord, ApiKeySummary, Operator, OperatorCredentialRecord, OperatorSessionRecord,
    },
    lifecycle::{LifecyclePhase, RequestLifecycle},
    state::AppState,
    storage::Storage,
};

pub const API_KEY_HEADER: &str = "x-garden-api-key";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AuthMode {
    #[default]
    Disabled,
    Optional,
    Required,
}

impl FromStr for AuthMode {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "disabled" => Ok(Self::Disabled),
            "optional" => Ok(Self::Optional),
            "required" => Ok(Self::Required),
            _ => {
                anyhow::bail!("GARDEN_RELAY_AUTH_MODE must be one of: disabled, optional, required")
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthContext {
    pub key_id: String,
    pub tenant_id: String,
    pub app_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatedApiKey {
    pub key: ApiKeySummary,
    pub secret: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperatorContext {
    pub session_id: String,
    pub operator: Operator,
    pub expires_at: i64,
}

#[derive(Debug, Clone)]
pub struct CreatedOperatorSession {
    pub secret: String,
    pub context: OperatorContext,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatedOperator {
    pub operator: Operator,
    pub secret: String,
}

#[derive(Debug, Clone)]
pub struct AuthService {
    storage: Storage,
    bootstrap_token: Option<String>,
    operator_session_ttl_seconds: i64,
    failed_attempts: Arc<Mutex<HashMap<String, VecDeque<i64>>>>,
}

impl AuthService {
    pub fn new(storage: Storage) -> Self {
        Self {
            storage,
            bootstrap_token: None,
            operator_session_ttl_seconds: 8 * 60 * 60,
            failed_attempts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_operator_config(
        mut self,
        bootstrap_token: Option<String>,
        session_ttl_seconds: i64,
    ) -> Self {
        self.bootstrap_token = bootstrap_token;
        self.operator_session_ttl_seconds = session_ttl_seconds.max(60);
        self
    }

    pub fn create_api_key(
        &self,
        tenant_id: &str,
        app_id: &str,
        name: &str,
        expires_at: Option<i64>,
    ) -> anyhow::Result<CreatedApiKey> {
        if name.trim().is_empty() {
            anyhow::bail!("API key name must not be empty");
        }
        if expires_at.is_some_and(|expiration| expiration <= unix_timestamp()) {
            anyhow::bail!("API key expiration must be in the future");
        }

        let prefix = uuid::Uuid::new_v4().simple().to_string()[..12].to_owned();
        let secret_part = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let secret = format!("gr_live_{prefix}_{secret_part}");
        let salt_value = uuid::Uuid::new_v4().simple().to_string();
        let salt = SaltString::from_b64(&salt_value)
            .map_err(|error| anyhow::anyhow!("failed to generate API key salt: {error}"))?;
        let secret_hash = Argon2::default()
            .hash_password(secret.as_bytes(), &salt)
            .map_err(|error| anyhow::anyhow!("failed to hash API key: {error}"))?
            .to_string();
        let record = ApiKeyRecord {
            id: format!("key_{}", uuid::Uuid::new_v4().simple()),
            prefix,
            secret_hash,
            tenant_id: tenant_id.to_owned(),
            app_id: app_id.to_owned(),
            name: name.trim().to_owned(),
            expires_at,
            revoked_at: None,
            last_used_at: None,
            created_at: unix_timestamp(),
        };
        self.storage.save_api_key(&record)?;
        Ok(CreatedApiKey {
            key: record.into(),
            secret,
        })
    }

    pub fn authenticate(&self, secret: &str) -> anyhow::Result<Option<AuthContext>> {
        if self.relay_rate_limited(secret) {
            return Ok(None);
        }
        let Some(prefix) = api_key_prefix(secret) else {
            self.record_relay_failure(secret);
            return Ok(None);
        };
        let Some(record) = self.storage.find_api_key_by_prefix(prefix)? else {
            self.record_relay_failure(secret);
            return Ok(None);
        };
        if record.revoked_at.is_some()
            || record
                .expires_at
                .is_some_and(|expiration| expiration <= unix_timestamp())
        {
            self.record_relay_failure(secret);
            return Ok(None);
        }
        let hash = PasswordHash::new(&record.secret_hash)
            .map_err(|error| anyhow::anyhow!("invalid stored API key hash: {error}"))?;
        if Argon2::default()
            .verify_password(secret.as_bytes(), &hash)
            .is_err()
        {
            self.record_relay_failure(secret);
            return Ok(None);
        }
        self.clear_relay_failures(secret);
        self.storage.mark_api_key_used(&record.id)?;
        Ok(Some(AuthContext {
            key_id: record.id,
            tenant_id: record.tenant_id,
            app_id: record.app_id,
        }))
    }

    pub fn relay_rate_limited(&self, secret: &str) -> bool {
        let key = failure_key(secret);
        let cutoff = unix_timestamp() - 60;
        self.failed_attempts
            .lock()
            .ok()
            .and_then(|mut failures| {
                let attempts = failures.get_mut(&key)?;
                attempts.retain(|timestamp| *timestamp > cutoff);
                Some(attempts.len() >= 10)
            })
            .unwrap_or(false)
    }

    fn record_relay_failure(&self, secret: &str) {
        if let Ok(mut failures) = self.failed_attempts.lock() {
            let attempts = failures.entry(failure_key(secret)).or_default();
            attempts.push_back(unix_timestamp());
            while attempts.len() > 10 {
                attempts.pop_front();
            }
        }
    }

    fn clear_relay_failures(&self, secret: &str) {
        if let Ok(mut failures) = self.failed_attempts.lock() {
            failures.remove(&failure_key(secret));
        }
    }

    fn named_rate_limited(&self, key: &str) -> bool {
        let cutoff = unix_timestamp() - 60;
        self.failed_attempts
            .lock()
            .ok()
            .and_then(|mut failures| {
                let attempts = failures.get_mut(key)?;
                attempts.retain(|timestamp| *timestamp > cutoff);
                Some(attempts.len() >= 10)
            })
            .unwrap_or(false)
    }

    fn record_named_failure(&self, key: &str) {
        if let Ok(mut failures) = self.failed_attempts.lock() {
            let attempts = failures.entry(key.to_owned()).or_default();
            attempts.push_back(unix_timestamp());
            while attempts.len() > 10 {
                attempts.pop_front();
            }
        }
    }

    fn clear_named_failures(&self, key: &str) {
        if let Ok(mut failures) = self.failed_attempts.lock() {
            failures.remove(key);
        }
    }

    pub fn login_operator(&self, bootstrap_token: &str) -> anyhow::Result<CreatedOperatorSession> {
        if self.named_rate_limited("operator_login") {
            anyhow::bail!("too many failed operator login attempts");
        }
        let operator = if self.bootstrap_token.as_deref().is_some_and(|configured| {
            constant_time_equal(configured.as_bytes(), bootstrap_token.as_bytes())
        }) {
            self.storage.get_or_create_bootstrap_operator()?
        } else {
            let Some(operator) = self.authenticate_operator_credential(bootstrap_token)? else {
                self.record_named_failure("operator_login");
                anyhow::bail!("invalid operator credentials");
            };
            operator
        };
        self.clear_named_failures("operator_login");
        let prefix = uuid::Uuid::new_v4().simple().to_string()[..12].to_owned();
        let secret_part = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let secret = format!("gr_session_{prefix}_{secret_part}");
        let record = OperatorSessionRecord {
            id: format!("session_{}", uuid::Uuid::new_v4().simple()),
            prefix,
            secret_hash: hash_secret(&secret)?,
            operator_id: operator.id.clone(),
            expires_at: unix_timestamp() + self.operator_session_ttl_seconds,
            revoked_at: None,
            created_at: unix_timestamp(),
        };
        self.storage.save_operator_session(&record)?;
        Ok(CreatedOperatorSession {
            secret,
            context: OperatorContext {
                session_id: record.id,
                operator,
                expires_at: record.expires_at,
            },
            expires_at: record.expires_at,
        })
    }

    pub fn authenticate_operator(&self, secret: &str) -> anyhow::Result<Option<OperatorContext>> {
        let Some(prefix) = operator_session_prefix(secret) else {
            return Ok(None);
        };
        let Some((session, operator)) = self.storage.find_operator_session_by_prefix(prefix)?
        else {
            return Ok(None);
        };
        if session.revoked_at.is_some() || session.expires_at <= unix_timestamp() {
            return Ok(None);
        }
        if !verify_secret(secret, &session.secret_hash)? {
            return Ok(None);
        }
        Ok(Some(OperatorContext {
            session_id: session.id,
            operator,
            expires_at: session.expires_at,
        }))
    }

    pub fn logout_operator(&self, session_id: &str) -> anyhow::Result<bool> {
        self.storage.revoke_operator_session(session_id)
    }

    pub fn create_operator(&self, name: &str) -> anyhow::Result<CreatedOperator> {
        let operator = self.storage.create_operator(name)?;
        let prefix = uuid::Uuid::new_v4().simple().to_string()[..12].to_owned();
        let secret = format!(
            "gr_operator_{}_{}{}",
            prefix,
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        self.storage
            .save_operator_credential(&OperatorCredentialRecord {
                id: format!("operator_key_{}", uuid::Uuid::new_v4().simple()),
                prefix,
                secret_hash: hash_secret(&secret)?,
                operator_id: operator.id.clone(),
                revoked_at: None,
                created_at: unix_timestamp(),
            })?;
        Ok(CreatedOperator { operator, secret })
    }

    fn authenticate_operator_credential(&self, secret: &str) -> anyhow::Result<Option<Operator>> {
        let Some(prefix) = operator_credential_prefix(secret) else {
            return Ok(None);
        };
        let Some((credential, operator)) =
            self.storage.find_operator_credential_by_prefix(prefix)?
        else {
            return Ok(None);
        };
        if credential.revoked_at.is_some() || !verify_secret(secret, &credential.secret_hash)? {
            return Ok(None);
        }
        Ok(Some(operator))
    }
}

pub async fn relay_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let supplied = request
        .headers()
        .get(API_KEY_HEADER)
        .and_then(|value| value.to_str().ok());
    let context = match (state.auth_mode, supplied) {
        (AuthMode::Disabled, _) => None,
        (AuthMode::Optional, None) => None,
        (AuthMode::Required, None) => {
            return Err(record_relay_auth_failure(&state, AuthError::unauthorized()));
        }
        (AuthMode::Optional | AuthMode::Required, Some(secret)) => {
            if state.auth.relay_rate_limited(secret) {
                return Err(record_relay_auth_failure(&state, AuthError::rate_limited()));
            }
            let authenticated = state
                .auth
                .authenticate(secret)
                .map_err(|_| AuthError::unavailable())?;
            match authenticated {
                Some(context) => Some(context),
                None => {
                    return Err(record_relay_auth_failure(&state, AuthError::unauthorized()));
                }
            }
        }
    };
    if let Some(context) = &context {
        if let Some(user_id) = request
            .headers()
            .get("x-garden-user")
            .and_then(|value| value.to_str().ok())
            && !state
                .storage
                .user_belongs_to_tenant(&context.tenant_id, user_id)
                .map_err(|_| AuthError::unavailable())?
        {
            return Err(record_relay_auth_failure(&state, AuthError::unauthorized()));
        }
        request.headers_mut().remove("x-garden-tenant");
        request.headers_mut().remove("x-garden-app");
        request.headers_mut().insert(
            "x-garden-tenant",
            HeaderValue::from_str(&context.tenant_id).map_err(|_| AuthError::unavailable())?,
        );
        request.headers_mut().insert(
            "x-garden-app",
            HeaderValue::from_str(&context.app_id).map_err(|_| AuthError::unavailable())?,
        );
    }
    if let Some(context) = context {
        request.extensions_mut().insert(context);
    }
    Ok(next.run(request).await)
}

fn record_relay_auth_failure(state: &AppState, error: AuthError) -> AuthError {
    let mut lifecycle = RequestLifecycle::new();
    lifecycle.record_phase(LifecyclePhase::RequestReceived);
    lifecycle.record_failure(error.status, error.code);
    state.save_lifecycle(lifecycle.snapshot());
    error
}

pub async fn operator_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let secret = request
        .headers()
        .get("cookie")
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| cookie_value(cookies, "garden_session"))
        .ok_or_else(AuthError::unauthorized)?;
    let context = state
        .auth
        .authenticate_operator(secret)
        .map_err(|_| AuthError::unavailable())?
        .ok_or_else(AuthError::unauthorized)?;
    request.extensions_mut().insert(context);
    Ok(next.run(request).await)
}

fn cookie_value<'a>(cookies: &'a str, name: &str) -> Option<&'a str> {
    cookies.split(';').find_map(|cookie| {
        let (candidate, value) = cookie.trim().split_once('=')?;
        (candidate == name).then_some(value)
    })
}

fn api_key_prefix(secret: &str) -> Option<&str> {
    let value = secret.strip_prefix("gr_live_")?;
    let (prefix, secret) = value.split_once('_')?;
    if prefix.len() == 12 && secret.len() == 64 {
        Some(prefix)
    } else {
        None
    }
}

fn operator_session_prefix(secret: &str) -> Option<&str> {
    let value = secret.strip_prefix("gr_session_")?;
    let (prefix, secret) = value.split_once('_')?;
    if prefix.len() == 12 && secret.len() == 64 {
        Some(prefix)
    } else {
        None
    }
}

fn operator_credential_prefix(secret: &str) -> Option<&str> {
    let value = secret.strip_prefix("gr_operator_")?;
    let (prefix, secret) = value.split_once('_')?;
    if prefix.len() == 12 && secret.len() == 64 {
        Some(prefix)
    } else {
        None
    }
}

fn hash_secret(secret: &str) -> anyhow::Result<String> {
    let salt_value = uuid::Uuid::new_v4().simple().to_string();
    let salt = SaltString::from_b64(&salt_value)
        .map_err(|error| anyhow::anyhow!("failed to generate credential salt: {error}"))?;
    Ok(Argon2::default()
        .hash_password(secret.as_bytes(), &salt)
        .map_err(|error| anyhow::anyhow!("failed to hash credential: {error}"))?
        .to_string())
}

fn verify_secret(secret: &str, encoded_hash: &str) -> anyhow::Result<bool> {
    let hash = PasswordHash::new(encoded_hash)
        .map_err(|error| anyhow::anyhow!("invalid stored credential hash: {error}"))?;
    Ok(Argon2::default()
        .verify_password(secret.as_bytes(), &hash)
        .is_ok())
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let length = left.len().max(right.len());
    for index in 0..length {
        difference |= usize::from(*left.get(index).unwrap_or(&0) ^ *right.get(index).unwrap_or(&0));
    }
    difference == 0
}

fn failure_key(secret: &str) -> String {
    api_key_prefix(secret).unwrap_or("malformed").to_owned()
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

#[derive(Debug)]
pub struct AuthError {
    status: StatusCode,
    code: &'static str,
    message: &'static str,
}

impl AuthError {
    fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "invalid_relay_credentials",
            message: "Valid Garden Relay credentials are required.",
        }
    }

    fn unavailable() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "authentication_unavailable",
            message: "Garden Relay authentication is temporarily unavailable.",
        }
    }

    fn rate_limited() -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            code: "authentication_rate_limited",
            message: "Too many failed authentication attempts.",
        }
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({
                "error": {
                    "type": self.code,
                    "code": self.code,
                    "message": self.message,
                }
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::Storage;

    use super::*;

    #[test]
    fn parses_auth_modes() {
        assert_eq!("disabled".parse::<AuthMode>().unwrap(), AuthMode::Disabled);
        assert_eq!("optional".parse::<AuthMode>().unwrap(), AuthMode::Optional);
        assert_eq!("required".parse::<AuthMode>().unwrap(), AuthMode::Required);
        assert!("other".parse::<AuthMode>().is_err());
    }

    #[test]
    fn rate_limits_repeated_failed_key_verification_by_prefix() {
        let auth = AuthService::new(Storage::in_memory().unwrap());
        let invalid =
            "gr_live_123456789abc_1234567890123456789012345678901234567890123456789012345678901234";
        for _ in 0..10 {
            assert!(auth.authenticate(invalid).unwrap().is_none());
        }
        assert!(auth.relay_rate_limited(invalid));
        assert!(!auth.relay_rate_limited(
            "gr_live_abcdefabcdef_1234567890123456789012345678901234567890123456789012345678901234"
        ));
    }

    #[test]
    fn creates_verifies_and_revokes_api_keys_without_storing_the_secret() {
        let storage = Storage::in_memory().expect("storage");
        let tenant = storage.create_tenant("Acme").expect("tenant");
        let app = storage.create_app(&tenant.id, "Support").expect("app");
        let auth = AuthService::new(storage.clone());

        let created = auth
            .create_api_key(&tenant.id, &app.id, "Production", None)
            .expect("key");
        let stored = storage
            .find_api_key_by_prefix(&created.key.prefix)
            .expect("lookup")
            .expect("stored key");

        assert_ne!(stored.secret_hash, created.secret);
        assert!(!stored.secret_hash.contains(&created.secret));
        assert!(
            auth.authenticate("gr_live_invalid_secret")
                .unwrap()
                .is_none()
        );
        assert_eq!(
            auth.authenticate(&created.secret).unwrap(),
            Some(AuthContext {
                key_id: created.key.id.clone(),
                tenant_id: tenant.id,
                app_id: app.id.clone(),
            })
        );

        let replacement = auth
            .create_api_key(&created.key.tenant_id, &app.id, "Replacement", None)
            .unwrap();
        assert!(storage.revoke_api_key(&app.id, &created.key.id).unwrap());
        assert!(auth.authenticate(&created.secret).unwrap().is_none());
        assert!(auth.authenticate(&replacement.secret).unwrap().is_some());

        let expired_secret =
            "gr_live_123456789abc_1234567890123456789012345678901234567890123456789012345678901234";
        storage
            .save_api_key(&ApiKeyRecord {
                id: "key_expired".to_owned(),
                prefix: "123456789abc".to_owned(),
                secret_hash: hash_secret(expired_secret).unwrap(),
                tenant_id: created.key.tenant_id.clone(),
                app_id: app.id.clone(),
                name: "Expired".to_owned(),
                expires_at: Some(unix_timestamp() - 1),
                revoked_at: None,
                last_used_at: None,
                created_at: unix_timestamp() - 10,
            })
            .unwrap();
        assert!(auth.authenticate(expired_secret).unwrap().is_none());

        storage
            .update_tenant(&created.key.tenant_id, None, Some(false))
            .unwrap();
        assert!(auth.authenticate(&replacement.secret).unwrap().is_none());
        let active_key = auth
            .create_api_key(&created.key.tenant_id, &app.id, "Inactive", None)
            .expect_err("inactive tenant cannot receive a key");
        assert!(active_key.to_string().contains("active"));
    }

    #[test]
    fn operator_sessions_expire_and_are_invalid_after_logout() {
        let storage = Storage::in_memory().expect("storage");
        let auth = AuthService::new(storage.clone())
            .with_operator_config(Some("bootstrap-secret".to_owned()), 3600);
        assert!(auth.login_operator("wrong-secret").is_err());

        let created = auth
            .login_operator("bootstrap-secret")
            .expect("operator login");
        assert_eq!(
            auth.authenticate_operator(&created.secret)
                .expect("authenticate")
                .expect("session")
                .operator
                .id,
            created.context.operator.id
        );
        assert!(auth.logout_operator(&created.context.session_id).unwrap());
        assert!(
            auth.authenticate_operator(&created.secret)
                .unwrap()
                .is_none()
        );

        let expired_secret = "gr_session_123456789abc_1234567890123456789012345678901234567890123456789012345678901234";
        let expired = OperatorSessionRecord {
            id: "session_expired".to_owned(),
            prefix: "123456789abc".to_owned(),
            secret_hash: hash_secret(expired_secret).unwrap(),
            operator_id: created.context.operator.id,
            expires_at: unix_timestamp() - 1,
            revoked_at: None,
            created_at: unix_timestamp() - 10,
        };
        storage.save_operator_session(&expired).unwrap();
        assert!(
            auth.authenticate_operator(expired_secret)
                .unwrap()
                .is_none()
        );

        let invited = auth.create_operator("Second Operator").unwrap();
        let invited_session = auth.login_operator(&invited.secret).unwrap();
        assert_eq!(invited_session.context.operator.id, invited.operator.id);
    }
}
