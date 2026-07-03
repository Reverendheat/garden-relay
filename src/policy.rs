use std::{
    fs,
    path::Path,
    sync::{Arc, RwLock},
};

use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};

use crate::domain::RelayRequest;

#[derive(Debug, Clone, Default)]
pub struct PolicyEngine {
    policies: Arc<RwLock<Vec<StaticPolicy>>>,
}

impl PolicyEngine {
    pub fn from_policies(policies: Vec<StaticPolicy>) -> Self {
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
            .find(|existing| existing.name == policy.name)
        {
            *existing = policy;
        } else {
            policies.push(policy);
        }

        policies.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(())
    }

    pub fn list_policies(&self) -> Vec<StaticPolicy> {
        match self.policies.read() {
            Ok(policies) => policies.clone(),
            Err(error) => {
                tracing::error!("policy store lock poisoned: {error}");
                Vec::new()
            }
        }
    }

    pub fn evaluate(&self, phase: PolicyPhase, ctx: &PolicyContext<'_>) -> Vec<PolicyDecision> {
        match self.policies.read() {
            Ok(policies) => policies
                .iter()
                .filter(|policy| policy.phase == phase)
                .map(|policy| policy.evaluate(ctx))
                .collect(),
            Err(error) => {
                tracing::error!("policy store lock poisoned: {error}");
                Vec::new()
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct PolicyContext<'a> {
    pub headers: &'a HeaderMap,
    pub request: &'a RelayRequest,
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
        self.effects
            .iter()
            .map(|effect| match effect {
                PolicyEffect::Deny { reason } => reason.as_str(),
            })
            .next()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
pub enum PolicyEffect {
    Deny { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyPhase {
    BeforeInput,
    BeforeModel,
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
            vec![self.action.to_effect()]
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
    pub model: Option<String>,
}

impl StaticCondition {
    fn has_any_condition(&self) -> bool {
        self.always.is_some() || self.missing_header.is_some() || self.model.is_some()
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

        if let Some(model) = &self.model
            && ctx.request.model != *model
        {
            return false;
        }

        true
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StaticAction {
    pub effect: StaticEffectKind,
    pub reason: Option<String>,
}

impl StaticAction {
    fn to_effect(&self) -> PolicyEffect {
        match self.effect {
            StaticEffectKind::Deny => PolicyEffect::Deny {
                reason: self
                    .reason
                    .clone()
                    .unwrap_or_else(|| "Request denied by policy.".to_owned()),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StaticEffectKind {
    Deny,
}

fn is_yaml_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "yaml" | "yml"))
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderMap;

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
            action: StaticAction {
                effect: StaticEffectKind::Deny,
                reason: Some("Tenant header required.".to_owned()),
            },
        };
        let engine = PolicyEngine::from_policies(vec![policy]);
        let headers = HeaderMap::new();
        let request = relay_request();
        let ctx = PolicyContext {
            headers: &headers,
            request: &request,
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
            action: StaticAction {
                effect: StaticEffectKind::Deny,
                reason: None,
            },
        };
        let engine = PolicyEngine::from_policies(vec![policy]);
        let mut headers = HeaderMap::new();
        headers.insert("x-garden-tenant", "tenant_123".parse().unwrap());
        let request = relay_request();
        let ctx = PolicyContext {
            headers: &headers,
            request: &request,
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
            action: StaticAction {
                effect: StaticEffectKind::Deny,
                reason: Some("first".to_owned()),
            },
        };
        let replacement = StaticPolicy {
            action: StaticAction {
                effect: StaticEffectKind::Deny,
                reason: Some("second".to_owned()),
            },
            ..policy.clone()
        };

        engine.add_policy(policy).expect("add policy");
        engine.add_policy(replacement).expect("replace policy");

        let policies = engine.list_policies();
        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].action.reason.as_deref(), Some("second"));
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
}
