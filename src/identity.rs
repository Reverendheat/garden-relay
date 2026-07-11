use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tenant {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct App {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct User {
    pub id: String,
    pub tenant_id: String,
    pub external_id: String,
    pub display_name: Option<String>,
    pub active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct ApiKeyRecord {
    pub id: String,
    pub prefix: String,
    pub secret_hash: String,
    pub tenant_id: String,
    pub app_id: String,
    pub name: String,
    pub expires_at: Option<i64>,
    pub revoked_at: Option<i64>,
    pub last_used_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiKeySummary {
    pub id: String,
    pub prefix: String,
    pub tenant_id: String,
    pub app_id: String,
    pub name: String,
    pub expires_at: Option<i64>,
    pub revoked_at: Option<i64>,
    pub last_used_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Operator {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct OperatorSessionRecord {
    pub id: String,
    pub prefix: String,
    pub secret_hash: String,
    pub operator_id: String,
    pub expires_at: i64,
    pub revoked_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct OperatorCredentialRecord {
    pub id: String,
    pub prefix: String,
    pub secret_hash: String,
    pub operator_id: String,
    pub revoked_at: Option<i64>,
    pub created_at: i64,
}

impl From<ApiKeyRecord> for ApiKeySummary {
    fn from(key: ApiKeyRecord) -> Self {
        Self {
            id: key.id,
            prefix: key.prefix,
            tenant_id: key.tenant_id,
            app_id: key.app_id,
            name: key.name,
            expires_at: key.expires_at,
            revoked_at: key.revoked_at,
            last_used_at: key.last_used_at,
            created_at: key.created_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantScope {
    pub tenant_id: String,
    pub app_id: Option<String>,
}

impl TenantScope {
    pub fn tenant(tenant_id: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            app_id: None,
        }
    }

    pub fn app(tenant_id: impl Into<String>, app_id: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            app_id: Some(app_id.into()),
        }
    }
}
