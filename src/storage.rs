use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, OptionalExtension, params, types::Type};

use crate::{
    identity::{
        ApiKeyRecord, ApiKeySummary, App, Operator, OperatorCredentialRecord,
        OperatorSessionRecord, Tenant, User,
    },
    lifecycle::{LifecycleOutcome, LifecycleSnapshot},
    policy::{ScopedPolicy, StaticPolicy},
};

#[derive(Debug, Clone)]
pub struct Storage {
    inner: Arc<Mutex<Connection>>,
}

impl Storage {
    pub fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let connection = Connection::open(path)?;
        let storage = Self {
            inner: Arc::new(Mutex::new(connection)),
        };
        storage.migrate()?;
        Ok(storage)
    }

    #[cfg(test)]
    pub fn in_memory() -> anyhow::Result<Self> {
        let storage = Self {
            inner: Arc::new(Mutex::new(Connection::open_in_memory()?)),
        };
        storage.migrate()?;
        Ok(storage)
    }

    pub fn save_policy(&self, policy: &StaticPolicy) -> anyhow::Result<()> {
        policy.validate()?;
        let now = unix_timestamp();
        let policy_json = serde_json::to_string(policy)?;
        let phase = serde_json::to_value(policy.phase)?
            .as_str()
            .unwrap_or("unknown")
            .to_owned();

        self.connection()?.execute(
            r#"
            INSERT INTO policies
                (policy_id, tenant_id, app_id, name, phase, policy_json, created_at, updated_at)
            VALUES (?1, NULL, NULL, ?2, ?3, ?4, ?5, ?5)
            ON CONFLICT DO UPDATE SET
                phase = excluded.phase,
                policy_json = excluded.policy_json,
                updated_at = excluded.updated_at
            "#,
            params![
                format!("policy_{}", uuid::Uuid::new_v4().simple()),
                policy.name,
                phase,
                policy_json,
                now
            ],
        )?;

        Ok(())
    }

    pub fn save_scoped_policy(&self, policy: &ScopedPolicy) -> anyhow::Result<ScopedPolicy> {
        policy.policy.validate()?;
        if policy.app_id.is_some() && policy.tenant_id.is_none() {
            anyhow::bail!("app-scoped policies must include a tenant");
        }
        if let Some(tenant_id) = &policy.tenant_id {
            self.require_active_tenant(tenant_id)?;
            if let Some(app_id) = &policy.app_id
                && !self.app_belongs_to_tenant(tenant_id, app_id)?
            {
                anyhow::bail!("policy app does not belong to policy tenant");
            }
        }
        let now = unix_timestamp();
        let policy_json = serde_json::to_string(&policy.policy)?;
        let phase = serde_json::to_value(policy.policy.phase)?
            .as_str()
            .unwrap_or("unknown")
            .to_owned();
        self.connection()?.execute(
            r#"INSERT INTO policies
               (policy_id, tenant_id, app_id, name, phase, policy_json, created_at, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
               ON CONFLICT(policy_id) DO UPDATE SET
                   tenant_id = excluded.tenant_id, app_id = excluded.app_id,
                   name = excluded.name, phase = excluded.phase,
                   policy_json = excluded.policy_json, updated_at = excluded.updated_at
               ON CONFLICT DO UPDATE SET phase = excluded.phase,
                   policy_json = excluded.policy_json, updated_at = excluded.updated_at"#,
            params![
                policy.policy_id,
                policy.tenant_id,
                policy.app_id,
                policy.policy.name,
                phase,
                policy_json,
                now
            ],
        )?;
        let policy_id = self.connection()?.query_row(
            r#"SELECT policy_id FROM policies
               WHERE ifnull(tenant_id, '') = ifnull(?1, '')
                 AND ifnull(app_id, '') = ifnull(?2, '') AND name = ?3"#,
            params![policy.tenant_id, policy.app_id, policy.policy.name],
            |row| row.get(0),
        )?;
        Ok(ScopedPolicy {
            policy_id,
            tenant_id: policy.tenant_id.clone(),
            app_id: policy.app_id.clone(),
            policy: policy.policy.clone(),
        })
    }

    pub fn list_scoped_policies(&self) -> anyhow::Result<Vec<ScopedPolicy>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"SELECT policy_id, tenant_id, app_id, policy_json FROM policies
               ORDER BY CASE WHEN tenant_id IS NULL THEN 0 WHEN app_id IS NULL THEN 1 ELSE 2 END,
                        name ASC"#,
        )?;
        statement
            .query_map([], |row| {
                let policy_json = row.get::<_, String>(3)?;
                Ok(ScopedPolicy {
                    policy_id: row.get(0)?,
                    tenant_id: row.get(1)?,
                    app_id: row.get(2)?,
                    policy: serde_json::from_str(&policy_json)
                        .map_err(|error| json_decode_error(3, error))?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn create_tenant(&self, name: &str) -> anyhow::Result<Tenant> {
        let name = required("tenant name", name)?;
        let now = unix_timestamp();
        let tenant = Tenant {
            id: format!("tenant_{}", uuid::Uuid::new_v4().simple()),
            name: name.to_owned(),
            active: true,
            created_at: now,
            updated_at: now,
        };
        self.connection()?.execute(
            "INSERT INTO tenants (id, name, active, created_at, updated_at) VALUES (?1, ?2, 1, ?3, ?3)",
            params![tenant.id, tenant.name, now],
        )?;
        Ok(tenant)
    }

    pub fn list_tenants(&self) -> anyhow::Result<Vec<Tenant>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, name, active, created_at, updated_at FROM tenants ORDER BY name",
        )?;
        statement
            .query_map([], |row| {
                Ok(Tenant {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    active: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn update_tenant(
        &self,
        tenant_id: &str,
        name: Option<&str>,
        active: Option<bool>,
    ) -> anyhow::Result<Option<Tenant>> {
        let name = name
            .map(|value| required("tenant name", value))
            .transpose()?;
        self.connection()?.execute(
            r#"UPDATE tenants SET name = COALESCE(?2, name), active = COALESCE(?3, active),
               updated_at = ?4 WHERE id = ?1"#,
            params![tenant_id, name, active, unix_timestamp()],
        )?;
        self.connection()?
            .query_row(
                "SELECT id, name, active, created_at, updated_at FROM tenants WHERE id = ?1",
                [tenant_id],
                |row| {
                    Ok(Tenant {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        active: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn create_app(&self, tenant_id: &str, name: &str) -> anyhow::Result<App> {
        let name = required("app name", name)?;
        self.require_active_tenant(tenant_id)?;
        let now = unix_timestamp();
        let app = App {
            id: format!("app_{}", uuid::Uuid::new_v4().simple()),
            tenant_id: tenant_id.to_owned(),
            name: name.to_owned(),
            active: true,
            created_at: now,
            updated_at: now,
        };
        self.connection()?.execute(
            "INSERT INTO apps (id, tenant_id, name, active, created_at, updated_at) VALUES (?1, ?2, ?3, 1, ?4, ?4)",
            params![app.id, app.tenant_id, app.name, now],
        )?;
        Ok(app)
    }

    pub fn list_apps(&self, tenant_id: &str) -> anyhow::Result<Vec<App>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, tenant_id, name, active, created_at, updated_at FROM apps WHERE tenant_id = ?1 ORDER BY name",
        )?;
        statement
            .query_map([tenant_id], |row| {
                Ok(App {
                    id: row.get(0)?,
                    tenant_id: row.get(1)?,
                    name: row.get(2)?,
                    active: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn create_user(
        &self,
        tenant_id: &str,
        external_id: &str,
        display_name: Option<&str>,
    ) -> anyhow::Result<User> {
        let external_id = required("user external ID", external_id)?;
        self.require_active_tenant(tenant_id)?;
        let now = unix_timestamp();
        let user = User {
            id: format!("user_{}", uuid::Uuid::new_v4().simple()),
            tenant_id: tenant_id.to_owned(),
            external_id: external_id.to_owned(),
            display_name: display_name
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            active: true,
            created_at: now,
            updated_at: now,
        };
        self.connection()?.execute(
            "INSERT INTO users (id, tenant_id, external_id, display_name, active, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)",
            params![user.id, user.tenant_id, user.external_id, user.display_name, now],
        )?;
        Ok(user)
    }

    pub fn list_users(&self, tenant_id: &str) -> anyhow::Result<Vec<User>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, tenant_id, external_id, display_name, active, created_at, updated_at FROM users WHERE tenant_id = ?1 ORDER BY external_id",
        )?;
        statement
            .query_map([tenant_id], |row| {
                Ok(User {
                    id: row.get(0)?,
                    tenant_id: row.get(1)?,
                    external_id: row.get(2)?,
                    display_name: row.get(3)?,
                    active: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn user_belongs_to_tenant(&self, tenant_id: &str, user_id: &str) -> anyhow::Result<bool> {
        Ok(self.connection()?.query_row(
            "SELECT EXISTS(SELECT 1 FROM users WHERE tenant_id = ?1 AND id = ?2 AND active = 1)",
            params![tenant_id, user_id],
            |row| row.get(0),
        )?)
    }

    pub fn save_api_key(&self, key: &ApiKeyRecord) -> anyhow::Result<()> {
        if !self.app_belongs_to_tenant(&key.tenant_id, &key.app_id)? {
            anyhow::bail!("active app does not belong to active tenant");
        }
        self.connection()?.execute(
            r#"INSERT INTO api_keys
               (id, prefix, secret_hash, tenant_id, app_id, name, expires_at, revoked_at, last_used_at, created_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"#,
            params![key.id, key.prefix, key.secret_hash, key.tenant_id, key.app_id, key.name, key.expires_at, key.revoked_at, key.last_used_at, key.created_at],
        )?;
        Ok(())
    }

    pub fn find_api_key_by_prefix(&self, prefix: &str) -> anyhow::Result<Option<ApiKeyRecord>> {
        self.connection()?
            .query_row(
                r#"SELECT api_keys.id, api_keys.prefix, api_keys.secret_hash,
                          api_keys.tenant_id, api_keys.app_id, api_keys.name,
                          api_keys.expires_at, api_keys.revoked_at,
                          api_keys.last_used_at, api_keys.created_at
                   FROM api_keys
                   JOIN tenants ON tenants.id = api_keys.tenant_id AND tenants.active = 1
                   JOIN apps ON apps.id = api_keys.app_id AND apps.active = 1
                   WHERE prefix = ?1"#,
                [prefix],
                api_key_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_api_keys(&self, app_id: &str) -> anyhow::Result<Vec<ApiKeySummary>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"SELECT id, prefix, secret_hash, tenant_id, app_id, name, expires_at,
                      revoked_at, last_used_at, created_at
               FROM api_keys WHERE app_id = ?1 ORDER BY created_at DESC"#,
        )?;
        statement
            .query_map([app_id], api_key_from_row)?
            .map(|result| result.map(ApiKeySummary::from))
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn mark_api_key_used(&self, key_id: &str) -> anyhow::Result<()> {
        self.connection()?.execute(
            "UPDATE api_keys SET last_used_at = ?2 WHERE id = ?1",
            params![key_id, unix_timestamp()],
        )?;
        Ok(())
    }

    pub fn revoke_api_key(&self, app_id: &str, key_id: &str) -> anyhow::Result<bool> {
        Ok(self.connection()?.execute(
            "UPDATE api_keys SET revoked_at = ?3 WHERE id = ?1 AND app_id = ?2 AND revoked_at IS NULL",
            params![key_id, app_id, unix_timestamp()],
        )? == 1)
    }

    pub fn get_or_create_bootstrap_operator(&self) -> anyhow::Result<Operator> {
        if let Some(operator) = self.connection()?.query_row(
            "SELECT id, name, active, created_at, updated_at FROM operators WHERE active = 1 ORDER BY created_at LIMIT 1",
            [],
            operator_from_row,
        ).optional()? {
            return Ok(operator);
        }
        let now = unix_timestamp();
        let operator = Operator {
            id: format!("operator_{}", uuid::Uuid::new_v4().simple()),
            name: "Administrator".to_owned(),
            active: true,
            created_at: now,
            updated_at: now,
        };
        self.connection()?.execute(
            "INSERT INTO operators (id, name, active, created_at, updated_at) VALUES (?1, ?2, 1, ?3, ?3)",
            params![operator.id, operator.name, now],
        )?;
        Ok(operator)
    }

    pub fn create_operator(&self, name: &str) -> anyhow::Result<Operator> {
        let name = required("operator name", name)?;
        let now = unix_timestamp();
        let operator = Operator {
            id: format!("operator_{}", uuid::Uuid::new_v4().simple()),
            name: name.to_owned(),
            active: true,
            created_at: now,
            updated_at: now,
        };
        self.connection()?.execute(
            "INSERT INTO operators (id, name, active, created_at, updated_at) VALUES (?1, ?2, 1, ?3, ?3)",
            params![operator.id, operator.name, now],
        )?;
        Ok(operator)
    }

    pub fn list_operators(&self) -> anyhow::Result<Vec<Operator>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, name, active, created_at, updated_at FROM operators ORDER BY name",
        )?;
        statement
            .query_map([], operator_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn deactivate_operator(&self, operator_id: &str) -> anyhow::Result<bool> {
        let connection = self.connection()?;
        let changed = connection.execute(
            "UPDATE operators SET active = 0, updated_at = ?2 WHERE id = ?1 AND active = 1",
            params![operator_id, unix_timestamp()],
        )?;
        if changed == 1 {
            connection.execute(
                "UPDATE operator_sessions SET revoked_at = ?2 WHERE operator_id = ?1 AND revoked_at IS NULL",
                params![operator_id, unix_timestamp()],
            )?;
            connection.execute(
                "UPDATE operator_credentials SET revoked_at = ?2 WHERE operator_id = ?1 AND revoked_at IS NULL",
                params![operator_id, unix_timestamp()],
            )?;
        }
        Ok(changed == 1)
    }

    pub fn save_operator_credential(
        &self,
        credential: &OperatorCredentialRecord,
    ) -> anyhow::Result<()> {
        self.connection()?.execute(
            r#"INSERT INTO operator_credentials
               (id, prefix, secret_hash, operator_id, revoked_at, created_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6)"#,
            params![
                credential.id,
                credential.prefix,
                credential.secret_hash,
                credential.operator_id,
                credential.revoked_at,
                credential.created_at
            ],
        )?;
        Ok(())
    }

    pub fn find_operator_credential_by_prefix(
        &self,
        prefix: &str,
    ) -> anyhow::Result<Option<(OperatorCredentialRecord, Operator)>> {
        self.connection()?
            .query_row(
                r#"SELECT c.id, c.prefix, c.secret_hash, c.operator_id, c.revoked_at, c.created_at,
                      o.id, o.name, o.active, o.created_at, o.updated_at
               FROM operator_credentials c
               JOIN operators o ON o.id = c.operator_id AND o.active = 1
               WHERE c.prefix = ?1"#,
                [prefix],
                |row| {
                    Ok((
                        OperatorCredentialRecord {
                            id: row.get(0)?,
                            prefix: row.get(1)?,
                            secret_hash: row.get(2)?,
                            operator_id: row.get(3)?,
                            revoked_at: row.get(4)?,
                            created_at: row.get(5)?,
                        },
                        Operator {
                            id: row.get(6)?,
                            name: row.get(7)?,
                            active: row.get(8)?,
                            created_at: row.get(9)?,
                            updated_at: row.get(10)?,
                        },
                    ))
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn save_operator_session(&self, session: &OperatorSessionRecord) -> anyhow::Result<()> {
        self.connection()?.execute(
            r#"INSERT INTO operator_sessions
               (id, prefix, secret_hash, operator_id, expires_at, revoked_at, created_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
            params![
                session.id,
                session.prefix,
                session.secret_hash,
                session.operator_id,
                session.expires_at,
                session.revoked_at,
                session.created_at
            ],
        )?;
        Ok(())
    }

    pub fn find_operator_session_by_prefix(
        &self,
        prefix: &str,
    ) -> anyhow::Result<Option<(OperatorSessionRecord, Operator)>> {
        self.connection()?
            .query_row(
                r#"SELECT s.id, s.prefix, s.secret_hash, s.operator_id, s.expires_at,
                      s.revoked_at, s.created_at,
                      o.id, o.name, o.active, o.created_at, o.updated_at
               FROM operator_sessions s
               JOIN operators o ON o.id = s.operator_id AND o.active = 1
               WHERE s.prefix = ?1"#,
                [prefix],
                |row| {
                    Ok((
                        OperatorSessionRecord {
                            id: row.get(0)?,
                            prefix: row.get(1)?,
                            secret_hash: row.get(2)?,
                            operator_id: row.get(3)?,
                            expires_at: row.get(4)?,
                            revoked_at: row.get(5)?,
                            created_at: row.get(6)?,
                        },
                        Operator {
                            id: row.get(7)?,
                            name: row.get(8)?,
                            active: row.get(9)?,
                            created_at: row.get(10)?,
                            updated_at: row.get(11)?,
                        },
                    ))
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn revoke_operator_session(&self, session_id: &str) -> anyhow::Result<bool> {
        Ok(self.connection()?.execute(
            "UPDATE operator_sessions SET revoked_at = ?2 WHERE id = ?1 AND revoked_at IS NULL",
            params![session_id, unix_timestamp()],
        )? == 1)
    }

    fn require_active_tenant(&self, tenant_id: &str) -> anyhow::Result<()> {
        let exists: bool = self.connection()?.query_row(
            "SELECT EXISTS(SELECT 1 FROM tenants WHERE id = ?1 AND active = 1)",
            [tenant_id],
            |row| row.get(0),
        )?;
        if !exists {
            anyhow::bail!("active tenant '{tenant_id}' not found");
        }
        Ok(())
    }

    fn app_belongs_to_tenant(&self, tenant_id: &str, app_id: &str) -> anyhow::Result<bool> {
        Ok(self.connection()?.query_row(
            r#"SELECT EXISTS(
                SELECT 1 FROM apps
                JOIN tenants ON tenants.id = apps.tenant_id
                WHERE apps.id = ?2 AND apps.tenant_id = ?1
                  AND apps.active = 1 AND tenants.active = 1
            )"#,
            params![tenant_id, app_id],
            |row| row.get(0),
        )?)
    }

    pub fn scope_exists(&self, tenant_id: &str, app_id: Option<&str>) -> anyhow::Result<bool> {
        match app_id {
            Some(app_id) => self.app_belongs_to_tenant(tenant_id, app_id),
            None => {
                let exists: bool = self.connection()?.query_row(
                    "SELECT EXISTS(SELECT 1 FROM tenants WHERE id = ?1 AND active = 1)",
                    [tenant_id],
                    |row| row.get(0),
                )?;
                Ok(exists)
            }
        }
    }

    pub fn save_lifecycle(&self, snapshot: &LifecycleSnapshot) -> anyhow::Result<()> {
        let now = unix_timestamp();
        let snapshot_json = serde_json::to_string(snapshot)?;
        let relay_request = snapshot.relay_request.as_ref();
        let status = snapshot.outcome.as_ref().map(outcome_status);
        let status_code = snapshot.outcome.as_ref().map(outcome_status_code);

        self.connection()?.execute(
            r#"
            INSERT INTO request_lifecycles (
                request_id,
                model,
                tenant_id,
                app_id,
                user_id,
                status,
                status_code,
                snapshot_json,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
            ON CONFLICT(request_id) DO UPDATE SET
                model = excluded.model,
                tenant_id = excluded.tenant_id,
                app_id = excluded.app_id,
                user_id = excluded.user_id,
                status = excluded.status,
                status_code = excluded.status_code,
                snapshot_json = excluded.snapshot_json,
                updated_at = excluded.updated_at
            "#,
            params![
                snapshot.request_id,
                relay_request.map(|request| request.model.as_str()),
                relay_request.and_then(|request| request.metadata.tenant_id.as_deref()),
                relay_request.and_then(|request| request.metadata.app_id.as_deref()),
                relay_request.and_then(|request| request.metadata.user_id.as_deref()),
                status,
                status_code,
                snapshot_json,
                now,
            ],
        )?;

        Ok(())
    }

    pub fn get_lifecycle(&self, request_id: &str) -> anyhow::Result<Option<LifecycleSnapshot>> {
        let snapshot_json = self
            .connection()?
            .query_row(
                "SELECT snapshot_json FROM request_lifecycles WHERE request_id = ?1",
                params![request_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        snapshot_json
            .map(|snapshot_json| serde_json::from_str::<LifecycleSnapshot>(&snapshot_json))
            .transpose()
            .map_err(Into::into)
    }

    pub fn list_lifecycles(&self) -> anyhow::Result<Vec<LifecycleSnapshot>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT snapshot_json FROM request_lifecycles ORDER BY updated_at DESC LIMIT 250",
        )?;
        let snapshots = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .map(|row| {
                let snapshot_json = row?;
                serde_json::from_str::<LifecycleSnapshot>(&snapshot_json)
                    .map_err(|error| json_decode_error(0, error))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(snapshots)
    }

    pub fn list_lifecycles_for_tenant(
        &self,
        tenant_id: &str,
    ) -> anyhow::Result<Vec<LifecycleSnapshot>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT snapshot_json FROM request_lifecycles WHERE tenant_id = ?1 ORDER BY updated_at DESC LIMIT 250",
        )?;
        statement
            .query_map([tenant_id], |row| row.get::<_, String>(0))?
            .map(|row| {
                let json = row?;
                serde_json::from_str(&json).map_err(|error| json_decode_error(0, error))
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn get_lifecycle_for_tenant(
        &self,
        tenant_id: &str,
        request_id: &str,
    ) -> anyhow::Result<Option<LifecycleSnapshot>> {
        let snapshot_json = self
            .connection()?
            .query_row(
                "SELECT snapshot_json FROM request_lifecycles WHERE request_id = ?1 AND tenant_id = ?2",
                params![request_id, tenant_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        snapshot_json
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(Into::into)
    }

    fn migrate(&self) -> anyhow::Result<()> {
        let mut connection = self.connection()?;
        connection.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS policies (
                name TEXT PRIMARY KEY,
                phase TEXT NOT NULL,
                policy_json TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS request_lifecycles (
                request_id TEXT PRIMARY KEY,
                model TEXT,
                tenant_id TEXT,
                app_id TEXT,
                user_id TEXT,
                status TEXT,
                status_code INTEGER,
                snapshot_json TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_request_lifecycles_updated_at
                ON request_lifecycles(updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_request_lifecycles_tenant_id
                ON request_lifecycles(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_request_lifecycles_model
                ON request_lifecycles(model);

            CREATE TABLE IF NOT EXISTS tenants (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                active INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS apps (
                id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL REFERENCES tenants(id),
                name TEXT NOT NULL,
                active INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                UNIQUE (tenant_id, name)
            );

            CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL REFERENCES tenants(id),
                external_id TEXT NOT NULL,
                display_name TEXT,
                active INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                UNIQUE (tenant_id, external_id)
            );

            CREATE TABLE IF NOT EXISTS api_keys (
                id TEXT PRIMARY KEY,
                prefix TEXT NOT NULL UNIQUE,
                secret_hash TEXT NOT NULL,
                tenant_id TEXT NOT NULL REFERENCES tenants(id),
                app_id TEXT NOT NULL REFERENCES apps(id),
                name TEXT NOT NULL,
                expires_at INTEGER,
                revoked_at INTEGER,
                last_used_at INTEGER,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS operators (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                active INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS operator_sessions (
                id TEXT PRIMARY KEY,
                prefix TEXT NOT NULL UNIQUE,
                secret_hash TEXT NOT NULL,
                operator_id TEXT NOT NULL REFERENCES operators(id),
                expires_at INTEGER NOT NULL,
                revoked_at INTEGER,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS operator_credentials (
                id TEXT PRIMARY KEY,
                prefix TEXT NOT NULL UNIQUE,
                secret_hash TEXT NOT NULL,
                operator_id TEXT NOT NULL REFERENCES operators(id),
                revoked_at INTEGER,
                created_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_apps_tenant_id ON apps(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_users_tenant_id ON users(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_api_keys_app_id ON api_keys(app_id);
            CREATE INDEX IF NOT EXISTS idx_operator_sessions_operator_id
                ON operator_sessions(operator_id);
            CREATE INDEX IF NOT EXISTS idx_operator_credentials_operator_id
                ON operator_credentials(operator_id);
            "#,
        )?;

        if !table_has_column(&connection, "policies", "policy_id")? {
            let transaction = connection.transaction()?;
            transaction.execute_batch(
                r#"
                CREATE TABLE policies_v2 (
                    policy_id TEXT PRIMARY KEY,
                    tenant_id TEXT REFERENCES tenants(id),
                    app_id TEXT REFERENCES apps(id),
                    name TEXT NOT NULL,
                    phase TEXT NOT NULL,
                    policy_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                INSERT INTO policies_v2
                    (policy_id, tenant_id, app_id, name, phase, policy_json, created_at, updated_at)
                    SELECT 'policy_' || lower(hex(randomblob(16))), NULL, NULL, name, phase, policy_json, created_at, updated_at
                    FROM policies;
                DROP TABLE policies;
                ALTER TABLE policies_v2 RENAME TO policies;
                CREATE UNIQUE INDEX idx_policies_scope_name
                    ON policies(ifnull(tenant_id, ''), ifnull(app_id, ''), name);
                CREATE INDEX idx_policies_tenant_id ON policies(tenant_id);
                CREATE INDEX idx_policies_app_id ON policies(app_id);
                "#,
            )?;
            transaction.commit()?;
        }
        Ok(())
    }

    fn connection(&self) -> anyhow::Result<MutexGuard<'_, Connection>> {
        self.inner
            .lock()
            .map_err(|error| anyhow::anyhow!("storage lock poisoned: {error}"))
    }
}

fn required<'a>(field: &str, value: &'a str) -> anyhow::Result<&'a str> {
    let value = value.trim();
    if value.is_empty() {
        anyhow::bail!("{field} must not be empty");
    }
    Ok(value)
}

fn api_key_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ApiKeyRecord> {
    Ok(ApiKeyRecord {
        id: row.get(0)?,
        prefix: row.get(1)?,
        secret_hash: row.get(2)?,
        tenant_id: row.get(3)?,
        app_id: row.get(4)?,
        name: row.get(5)?,
        expires_at: row.get(6)?,
        revoked_at: row.get(7)?,
        last_used_at: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn operator_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Operator> {
    Ok(Operator {
        id: row.get(0)?,
        name: row.get(1)?,
        active: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

fn table_has_column(connection: &Connection, table: &str, column: &str) -> anyhow::Result<bool> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(columns.iter().any(|candidate| candidate == column))
}

fn outcome_status(outcome: &LifecycleOutcome) -> &'static str {
    match outcome {
        LifecycleOutcome::Completed { .. } => "completed",
        LifecycleOutcome::Failed { .. } => "failed",
    }
}

fn outcome_status_code(outcome: &LifecycleOutcome) -> u16 {
    match outcome {
        LifecycleOutcome::Completed { status_code }
        | LifecycleOutcome::Failed { status_code, .. } => *status_code,
    }
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn json_decode_error(column: usize, error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(column, Type::Text, Box::new(error))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use crate::{
        domain::{
            Message, MessageContent, MessageRole, RelayOperation, RelayOptions, RelayRequest,
            RequestMetadata,
        },
        lifecycle::{LifecyclePhase, RequestLifecycle},
        policy::{PolicyPhase, StaticAction, StaticCondition, StaticEffect, StaticEffectKind},
    };

    use super::*;

    #[test]
    fn persists_policies_by_name() {
        let storage = Storage::in_memory().expect("storage");
        let policy = policy("require_tenant");

        storage.save_policy(&policy).expect("save policy");

        let policies = storage.list_scoped_policies().expect("list policies");
        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].policy.name, "require_tenant");
    }

    #[test]
    fn persists_lifecycles() {
        let storage = Storage::in_memory().expect("storage");
        let mut lifecycle = RequestLifecycle::new();
        lifecycle.record_phase(LifecyclePhase::BeforeModel);
        lifecycle.record_failure(StatusCode::FORBIDDEN, "policy_denied");
        let snapshot = lifecycle.snapshot();

        storage.save_lifecycle(&snapshot).expect("save lifecycle");

        assert_eq!(
            storage
                .get_lifecycle(&snapshot.request_id)
                .expect("get lifecycle")
                .expect("lifecycle exists")
                .request_id,
            snapshot.request_id
        );
    }

    #[test]
    fn isolates_apps_and_users_by_tenant() {
        let storage = Storage::in_memory().expect("storage");
        let first = storage.create_tenant("First").expect("first tenant");
        let second = storage.create_tenant("Second").expect("second tenant");
        let app = storage.create_app(&first.id, "App").expect("app");
        let user = storage
            .create_user(&first.id, "customer-1", Some("Customer"))
            .expect("user");

        assert_eq!(storage.list_apps(&first.id).unwrap(), vec![app]);
        assert!(storage.list_apps(&second.id).unwrap().is_empty());
        assert_eq!(storage.list_users(&first.id).unwrap(), vec![user.clone()]);
        assert!(storage.list_users(&second.id).unwrap().is_empty());
        assert!(storage.user_belongs_to_tenant(&first.id, &user.id).unwrap());
        assert!(
            !storage
                .user_belongs_to_tenant(&second.id, &user.id)
                .unwrap()
        );
    }

    #[test]
    fn stores_same_policy_name_in_independent_tenant_scopes() {
        let storage = Storage::in_memory().unwrap();
        let first = storage.create_tenant("First").unwrap();
        let second = storage.create_tenant("Second").unwrap();
        for tenant in [&first, &second] {
            storage
                .save_scoped_policy(&ScopedPolicy {
                    policy_id: format!("policy_{}", tenant.id),
                    tenant_id: Some(tenant.id.clone()),
                    app_id: None,
                    policy: policy("shared-name"),
                })
                .unwrap();
        }
        let policies = storage.list_scoped_policies().unwrap();
        assert_eq!(policies.len(), 2);
        assert_ne!(policies[0].tenant_id, policies[1].tenant_id);

        let original = policies[0].clone();
        let renamed = storage
            .save_scoped_policy(&ScopedPolicy {
                policy: policy("renamed"),
                ..original.clone()
            })
            .unwrap();
        assert_eq!(renamed.policy_id, original.policy_id);
        assert_eq!(renamed.policy.name, "renamed");
        assert!(
            storage
                .list_scoped_policies()
                .unwrap()
                .iter()
                .any(|policy| policy.policy_id == original.policy_id
                    && policy.policy.name == "renamed")
        );
    }

    #[test]
    fn tenant_lifecycle_queries_do_not_cross_boundaries() {
        let storage = Storage::in_memory().unwrap();
        let mut ids = Vec::new();
        for tenant_id in ["tenant_first", "tenant_second"] {
            let mut lifecycle = RequestLifecycle::new();
            lifecycle.set_relay_request(RelayRequest {
                operation: RelayOperation::ChatCompletion,
                model: "test".to_owned(),
                messages: vec![Message {
                    role: MessageRole::User,
                    content: MessageContent::Text("hello".to_owned()),
                }],
                options: RelayOptions {
                    max_tokens: None,
                    temperature: None,
                },
                metadata: RequestMetadata {
                    tenant_id: Some(tenant_id.to_owned()),
                    app_id: None,
                    user_id: None,
                    provider_metadata: None,
                },
            });
            lifecycle.record_failure(StatusCode::FORBIDDEN, "test");
            let snapshot = lifecycle.snapshot();
            ids.push(snapshot.request_id.clone());
            storage.save_lifecycle(&snapshot).unwrap();
        }

        let first = storage.list_lifecycles_for_tenant("tenant_first").unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].request_id, ids[0]);
        assert!(
            storage
                .get_lifecycle_for_tenant("tenant_first", &ids[1])
                .unwrap()
                .is_none()
        );
    }

    fn policy(name: &str) -> StaticPolicy {
        StaticPolicy {
            name: name.to_owned(),
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
