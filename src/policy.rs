use std::{
    fs,
    path::Path,
    sync::{Arc, RwLock},
};

use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{domain::RelayRequest, identity::TenantScope};

#[derive(Debug, Clone, Default)]
pub struct PolicyEngine {
    policies: Arc<RwLock<Vec<ScopedPolicy>>>,
}

impl PolicyEngine {
    pub fn from_policies(policies: Vec<StaticPolicy>) -> Self {
        Self::from_scoped_policies(policies.into_iter().map(ScopedPolicy::global).collect())
    }

    pub fn from_scoped_policies(mut policies: Vec<ScopedPolicy>) -> Self {
        policies.sort_by(ScopedPolicy::compare);
        Self {
            policies: Arc::new(RwLock::new(policies)),
        }
    }

    pub fn from_dir(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::default());
        }

        let mut policies = Vec::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            if !is_yaml_file(&path) {
                continue;
            }

            let contents = fs::read_to_string(&path)?;
            let policy = serde_yaml::from_str::<StaticPolicy>(&contents).map_err(|error| {
                anyhow::anyhow!("failed to load policy {}: {error}", path.display())
            })?;
            policy.validate()?;
            policies.push(policy);
        }

        policies.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(Self::from_policies(policies))
    }

    pub fn add_policy(&self, policy: StaticPolicy) -> anyhow::Result<()> {
        policy.validate()?;
        let mut policies = self
            .policies
            .write()
            .map_err(|error| anyhow::anyhow!("policy store lock poisoned: {error}"))?;

        if let Some(existing) = policies
            .iter_mut()
            .find(|existing| existing.is_global() && existing.policy.name == policy.name)
        {
            existing.policy = policy;
        } else {
            policies.push(ScopedPolicy::global(policy));
        }

        policies.sort_by(ScopedPolicy::compare);
        Ok(())
    }

    pub fn add_scoped_policy(&self, policy: ScopedPolicy) -> anyhow::Result<()> {
        policy.policy.validate()?;
        policy.validate_scope()?;
        let mut policies = self
            .policies
            .write()
            .map_err(|error| anyhow::anyhow!("policy store lock poisoned: {error}"))?;
        if let Some(existing) = policies.iter_mut().find(|existing| {
            existing.tenant_id == policy.tenant_id
                && existing.app_id == policy.app_id
                && existing.policy.name == policy.policy.name
        }) {
            *existing = policy;
        } else {
            policies.push(policy);
        }
        policies.sort_by(ScopedPolicy::compare);
        Ok(())
    }

    pub fn list_policies(&self) -> Vec<StaticPolicy> {
        match self.policies.read() {
            Ok(policies) => policies
                .iter()
                .filter(|policy| policy.is_global())
                .map(|policy| policy.policy.clone())
                .collect(),
            Err(error) => {
                tracing::error!("policy store lock poisoned: {error}");
                Vec::new()
            }
        }
    }

    pub fn evaluate(&self, phase: PolicyPhase, ctx: &PolicyContext<'_>) -> Vec<PolicyDecision> {
        self.evaluate_scoped(phase, ctx, None)
    }

    pub fn evaluate_scoped(
        &self,
        phase: PolicyPhase,
        ctx: &PolicyContext<'_>,
        scope: Option<&TenantScope>,
    ) -> Vec<PolicyDecision> {
        match self.policies.read() {
            Ok(policies) => policies
                .iter()
                .filter(|policy| policy.applies_to(scope) && policy.policy.phase == phase)
                .map(|policy| policy.policy.evaluate(ctx))
                .collect(),
            Err(error) => {
                tracing::error!("policy store lock poisoned: {error}");
                Vec::new()
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ScopedPolicy {
    pub policy_id: String,
    pub tenant_id: Option<String>,
    pub app_id: Option<String>,
    pub policy: StaticPolicy,
}

impl ScopedPolicy {
    pub fn global(policy: StaticPolicy) -> Self {
        Self {
            policy_id: format!("policy_{}", uuid::Uuid::new_v4().simple()),
            tenant_id: None,
            app_id: None,
            policy,
        }
    }

    fn is_global(&self) -> bool {
        self.tenant_id.is_none() && self.app_id.is_none()
    }

    fn validate_scope(&self) -> anyhow::Result<()> {
        if self.app_id.is_some() && self.tenant_id.is_none() {
            anyhow::bail!("app-scoped policies must include a tenant");
        }
        Ok(())
    }

    fn applies_to(&self, scope: Option<&TenantScope>) -> bool {
        if self.is_global() {
            return true;
        }
        scope.is_some_and(|scope| {
            self.tenant_id.as_deref() == Some(scope.tenant_id.as_str())
                && self
                    .app_id
                    .as_ref()
                    .is_none_or(|app_id| scope.app_id.as_deref() == Some(app_id))
        })
    }

    fn compare(left: &Self, right: &Self) -> std::cmp::Ordering {
        left.rank()
            .cmp(&right.rank())
            .then_with(|| left.policy.name.cmp(&right.policy.name))
    }

    fn rank(&self) -> u8 {
        match (&self.tenant_id, &self.app_id) {
            (None, None) => 0,
            (Some(_), None) => 1,
            (Some(_), Some(_)) => 2,
            (None, Some(_)) => 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PolicyContext<'a> {
    pub headers: &'a HeaderMap,
    pub request: &'a RelayRequest,
    pub request_body: &'a Value,
    pub response_body: Option<&'a Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PolicyDecision {
    pub policy_name: String,
    pub phase: PolicyPhase,
    pub matched: bool,
    pub effects: Vec<PolicyEffect>,
}

impl PolicyDecision {
    pub fn deny_reason(&self) -> Option<&str> {
        self.effects.iter().find_map(|effect| match effect {
            PolicyEffect::Deny { reason } => Some(reason.as_str()),
            _ => None,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
pub enum PolicyEffect {
    Deny {
        reason: String,
    },
    Log {
        level: LogLevel,
        message: String,
    },
    DisableTools {
        tools: Vec<String>,
    },
    Augment {
        mode: AugmentMode,
        messages: Vec<PolicyMessage>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyPhase {
    BeforeInput,
    BeforeModel,
    AfterModel,
    BeforeResponse,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StaticPolicy {
    pub name: String,
    pub phase: PolicyPhase,
    #[serde(rename = "if")]
    pub condition: StaticCondition,
    #[serde(rename = "then")]
    pub action: StaticAction,
}

impl StaticPolicy {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.name.trim().is_empty() {
            anyhow::bail!("policy name must not be empty");
        }

        if !self.condition.has_any_condition() {
            anyhow::bail!("policy '{}' must define at least one condition", self.name);
        }

        Ok(())
    }

    fn evaluate(&self, ctx: &PolicyContext<'_>) -> PolicyDecision {
        let matched = self.condition.matches(ctx);
        let effects = if matched {
            self.action.to_effects()
        } else {
            Vec::new()
        };

        PolicyDecision {
            policy_name: self.name.clone(),
            phase: self.phase,
            matched,
            effects,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct StaticCondition {
    pub always: Option<bool>,
    pub missing_header: Option<String>,
    pub header_equals: Option<HeaderEqualsCondition>,
    pub model: Option<String>,
    pub tenant_id: Option<String>,
    pub app_id: Option<String>,
    pub user_id: Option<String>,
    pub input_contains: Option<String>,
    pub request_json_equals: Option<JsonEqualsCondition>,
    pub response_contains: Option<String>,
    pub response_json_equals: Option<JsonEqualsCondition>,
    pub estimated_input_tokens_greater_than: Option<usize>,
    pub tool_name: Option<String>,
}

impl StaticCondition {
    fn has_any_condition(&self) -> bool {
        self.always.is_some()
            || self.missing_header.is_some()
            || self.header_equals.is_some()
            || self.model.is_some()
            || self.tenant_id.is_some()
            || self.app_id.is_some()
            || self.user_id.is_some()
            || self.input_contains.is_some()
            || self.request_json_equals.is_some()
            || self.response_contains.is_some()
            || self.response_json_equals.is_some()
            || self.estimated_input_tokens_greater_than.is_some()
            || self.tool_name.is_some()
    }

    fn matches(&self, ctx: &PolicyContext<'_>) -> bool {
        if !self.has_any_condition() {
            return false;
        }

        if self.always == Some(false) {
            return false;
        }

        if let Some(header_name) = &self.missing_header
            && ctx.headers.contains_key(header_name)
        {
            return false;
        }

        if let Some(header_equals) = &self.header_equals
            && !header_equals.matches(ctx.headers)
        {
            return false;
        }

        if let Some(model) = &self.model
            && ctx.request.model != *model
        {
            return false;
        }

        if let Some(tenant_id) = &self.tenant_id
            && ctx.request.metadata.tenant_id.as_deref() != Some(tenant_id)
        {
            return false;
        }

        if let Some(app_id) = &self.app_id
            && ctx.request.metadata.app_id.as_deref() != Some(app_id)
        {
            return false;
        }

        if let Some(user_id) = &self.user_id
            && ctx.request.metadata.user_id.as_deref() != Some(user_id)
        {
            return false;
        }

        if let Some(needle) = &self.input_contains
            && !ctx.request.input_text().contains(needle)
        {
            return false;
        }

        if let Some(json_equals) = &self.request_json_equals
            && !json_equals.matches(ctx.request_body)
        {
            return false;
        }

        if let Some(needle) = &self.response_contains
            && !ctx
                .response_body
                .is_some_and(|response| json_contains_text(response, needle))
        {
            return false;
        }

        if let Some(json_equals) = &self.response_json_equals
            && !ctx
                .response_body
                .is_some_and(|response| json_equals.matches(response))
        {
            return false;
        }

        if let Some(threshold) = self.estimated_input_tokens_greater_than
            && ctx.request.estimated_input_tokens() <= threshold
        {
            return false;
        }

        if let Some(tool_name) = &self.tool_name
            && !request_has_tool_name(ctx.request_body, tool_name)
        {
            return false;
        }

        true
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HeaderEqualsCondition {
    pub name: String,
    pub value: String,
}

impl HeaderEqualsCondition {
    fn matches(&self, headers: &HeaderMap) -> bool {
        headers
            .get(&self.name)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value == self.value)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonEqualsCondition {
    pub pointer: String,
    pub value: Value,
}

impl JsonEqualsCondition {
    fn matches(&self, value: &Value) -> bool {
        value.pointer(&self.pointer) == Some(&self.value)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StaticAction {
    Single(StaticEffect),
    Multiple { effects: Vec<StaticEffect> },
}

impl StaticAction {
    fn to_effects(&self) -> Vec<PolicyEffect> {
        match self {
            Self::Single(effect) => vec![effect.to_effect()],
            Self::Multiple { effects } => effects.iter().map(StaticEffect::to_effect).collect(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StaticEffect {
    pub effect: StaticEffectKind,
    pub reason: Option<String>,
    pub level: Option<LogLevel>,
    pub message: Option<String>,
    pub tools: Option<Vec<String>>,
    pub mode: Option<AugmentMode>,
    pub messages: Option<Vec<PolicyMessage>>,
}

impl StaticEffect {
    fn to_effect(&self) -> PolicyEffect {
        match self.effect {
            StaticEffectKind::Deny => PolicyEffect::Deny {
                reason: self
                    .reason
                    .clone()
                    .unwrap_or_else(|| "Request denied by policy.".to_owned()),
            },
            StaticEffectKind::Log => PolicyEffect::Log {
                level: self.level.unwrap_or(LogLevel::Info),
                message: self
                    .message
                    .clone()
                    .or_else(|| self.reason.clone())
                    .unwrap_or_else(|| "Policy matched.".to_owned()),
            },
            StaticEffectKind::DisableTools => PolicyEffect::DisableTools {
                tools: self.tools.clone().unwrap_or_default(),
            },
            StaticEffectKind::Augment => PolicyEffect::Augment {
                mode: self.mode.unwrap_or(AugmentMode::Append),
                messages: self.messages.clone().unwrap_or_default(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StaticEffectKind {
    Deny,
    Log,
    DisableTools,
    Augment,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AugmentMode {
    Append,
    Prepend,
    Replace,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PolicyMessage {
    pub role: String,
    pub content: String,
}

fn is_yaml_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "yaml" | "yml"))
}

fn json_contains_text(value: &Value, needle: &str) -> bool {
    match value {
        Value::String(text) => text.contains(needle),
        Value::Array(values) => values.iter().any(|value| json_contains_text(value, needle)),
        Value::Object(values) => values
            .values()
            .any(|value| json_contains_text(value, needle)),
        _ => false,
    }
}

fn request_has_tool_name(body: &Value, name: &str) -> bool {
    body.get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| {
            tools.iter().any(|tool| {
                tool.pointer("/function/name")
                    .and_then(Value::as_str)
                    .is_some_and(|tool_name| tool_name == name)
            })
        })
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderMap;
    use serde_json::json;

    use crate::domain::{
        Message, MessageContent, MessageRole, RelayOperation, RelayOptions, RequestMetadata,
    };

    use super::*;

    #[test]
    fn deny_policy_matches_missing_header() {
        let policy = StaticPolicy {
            name: "require_tenant".to_owned(),
            phase: PolicyPhase::BeforeModel,
            condition: StaticCondition {
                missing_header: Some("x-garden-tenant".to_owned()),
                ..StaticCondition::default()
            },
            action: deny_action(Some("Tenant header required.")),
        };
        let engine = PolicyEngine::from_policies(vec![policy]);
        let headers = HeaderMap::new();
        let request = relay_request();
        let ctx = PolicyContext {
            headers: &headers,
            request: &request,
            request_body: &json!({ "model": "gpt-4.1-mini" }),
            response_body: None,
        };

        let decisions = engine.evaluate(PolicyPhase::BeforeModel, &ctx);

        assert_eq!(decisions.len(), 1);
        assert!(decisions[0].matched);
        assert_eq!(decisions[0].deny_reason(), Some("Tenant header required."));
    }

    #[test]
    fn missing_header_policy_does_not_match_when_header_exists() {
        let policy = StaticPolicy {
            name: "require_tenant".to_owned(),
            phase: PolicyPhase::BeforeModel,
            condition: StaticCondition {
                missing_header: Some("x-garden-tenant".to_owned()),
                ..StaticCondition::default()
            },
            action: deny_action(None),
        };
        let engine = PolicyEngine::from_policies(vec![policy]);
        let mut headers = HeaderMap::new();
        headers.insert("x-garden-tenant", "tenant_123".parse().unwrap());
        let request = relay_request();
        let ctx = PolicyContext {
            headers: &headers,
            request: &request,
            request_body: &json!({ "model": "gpt-4.1-mini" }),
            response_body: None,
        };

        let decisions = engine.evaluate(PolicyPhase::BeforeModel, &ctx);

        assert_eq!(decisions.len(), 1);
        assert!(!decisions[0].matched);
        assert!(decisions[0].effects.is_empty());
    }

    #[test]
    fn add_policy_upserts_by_name() {
        let engine = PolicyEngine::default();
        let policy = StaticPolicy {
            name: "require_tenant".to_owned(),
            phase: PolicyPhase::BeforeModel,
            condition: StaticCondition {
                missing_header: Some("x-garden-tenant".to_owned()),
                ..StaticCondition::default()
            },
            action: deny_action(Some("first")),
        };
        let replacement = StaticPolicy {
            action: deny_action(Some("second")),
            ..policy.clone()
        };

        engine.add_policy(policy).expect("add policy");
        engine.add_policy(replacement).expect("replace policy");

        let policies = engine.list_policies();
        assert_eq!(policies.len(), 1);
        let effects = policies[0].action.to_effects();
        assert!(matches!(
            &effects[0],
            PolicyEffect::Deny { reason } if reason == "second"
        ));
    }

    #[test]
    fn matches_header_tenant_input_request_json_tokens_and_tool() {
        let policy = StaticPolicy {
            name: "compound".to_owned(),
            phase: PolicyPhase::BeforeInput,
            condition: StaticCondition {
                header_equals: Some(HeaderEqualsCondition {
                    name: "x-garden-app".to_owned(),
                    value: "support_bot".to_owned(),
                }),
                tenant_id: Some("tenant_123".to_owned()),
                app_id: Some("support_bot".to_owned()),
                user_id: Some("user_456".to_owned()),
                input_contains: Some("secret".to_owned()),
                request_json_equals: Some(JsonEqualsCondition {
                    pointer: "/metadata/risk".to_owned(),
                    value: json!("high"),
                }),
                estimated_input_tokens_greater_than: Some(1),
                tool_name: Some("delete_file".to_owned()),
                ..StaticCondition::default()
            },
            action: deny_action(None),
        };
        let engine = PolicyEngine::from_policies(vec![policy]);
        let mut headers = HeaderMap::new();
        headers.insert("x-garden-app", "support_bot".parse().unwrap());
        headers.insert("x-garden-tenant", "tenant_123".parse().unwrap());
        headers.insert("x-garden-user", "user_456".parse().unwrap());
        let request_body = json!({
            "metadata": { "risk": "high" },
            "tools": [{
                "type": "function",
                "function": { "name": "delete_file" }
            }]
        });
        let request = RelayRequest {
            metadata: RequestMetadata {
                tenant_id: Some("tenant_123".to_owned()),
                app_id: Some("support_bot".to_owned()),
                user_id: Some("user_456".to_owned()),
                provider_metadata: None,
            },
            messages: vec![Message {
                role: MessageRole::User,
                content: MessageContent::Text("contains secret".to_owned()),
            }],
            ..relay_request()
        };
        let ctx = PolicyContext {
            headers: &headers,
            request: &request,
            request_body: &request_body,
            response_body: None,
        };

        let decisions = engine.evaluate(PolicyPhase::BeforeInput, &ctx);

        assert!(decisions[0].matched);
    }

    #[test]
    fn matches_response_content_and_json() {
        let policy = StaticPolicy {
            name: "block_response".to_owned(),
            phase: PolicyPhase::AfterModel,
            condition: StaticCondition {
                response_contains: Some("unsupported claim".to_owned()),
                response_json_equals: Some(JsonEqualsCondition {
                    pointer: "/choices/0/finish_reason".to_owned(),
                    value: json!("stop"),
                }),
                ..StaticCondition::default()
            },
            action: deny_action(None),
        };
        let engine = PolicyEngine::from_policies(vec![policy]);
        let headers = HeaderMap::new();
        let request = relay_request();
        let request_body = json!({});
        let response_body = json!({
            "choices": [{
                "finish_reason": "stop",
                "message": { "content": "unsupported claim" }
            }]
        });
        let ctx = PolicyContext {
            headers: &headers,
            request: &request,
            request_body: &request_body,
            response_body: Some(&response_body),
        };

        let decisions = engine.evaluate(PolicyPhase::AfterModel, &ctx);

        assert!(decisions[0].matched);
    }

    #[test]
    fn scoped_policies_resolve_global_then_tenant_then_app() {
        let make_policy = |name: &str| StaticPolicy {
            name: name.to_owned(),
            phase: PolicyPhase::BeforeModel,
            condition: StaticCondition {
                always: Some(true),
                ..StaticCondition::default()
            },
            action: deny_action(Some(name)),
        };
        let engine = PolicyEngine::from_scoped_policies(vec![
            ScopedPolicy::global(make_policy("global")),
            ScopedPolicy {
                policy_id: "tenant-policy".to_owned(),
                tenant_id: Some("tenant_1".to_owned()),
                app_id: None,
                policy: make_policy("tenant"),
            },
            ScopedPolicy {
                policy_id: "app-policy".to_owned(),
                tenant_id: Some("tenant_1".to_owned()),
                app_id: Some("app_1".to_owned()),
                policy: make_policy("app"),
            },
            ScopedPolicy {
                policy_id: "other-policy".to_owned(),
                tenant_id: Some("tenant_2".to_owned()),
                app_id: None,
                policy: make_policy("other"),
            },
        ]);
        let headers = HeaderMap::new();
        let request = relay_request();
        let request_body = json!({});
        let ctx = PolicyContext {
            headers: &headers,
            request: &request,
            request_body: &request_body,
            response_body: None,
        };

        let decisions = engine.evaluate_scoped(
            PolicyPhase::BeforeModel,
            &ctx,
            Some(&TenantScope::app("tenant_1", "app_1")),
        );
        assert_eq!(
            decisions
                .iter()
                .map(|decision| decision.policy_name.as_str())
                .collect::<Vec<_>>(),
            vec!["global", "tenant", "app"]
        );
    }

    fn relay_request() -> RelayRequest {
        RelayRequest {
            operation: RelayOperation::ChatCompletion,
            model: "gpt-4.1-mini".to_owned(),
            messages: vec![Message {
                role: MessageRole::User,
                content: MessageContent::Text("hello".to_owned()),
            }],
            options: RelayOptions {
                max_tokens: None,
                temperature: None,
            },
            metadata: RequestMetadata::default(),
        }
    }

    fn deny_action(reason: Option<&str>) -> StaticAction {
        StaticAction::Single(StaticEffect {
            effect: StaticEffectKind::Deny,
            reason: reason.map(ToOwned::to_owned),
            level: None,
            message: None,
            tools: None,
            mode: None,
            messages: None,
        })
    }
}
