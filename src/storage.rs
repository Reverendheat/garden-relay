use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, OptionalExtension, params, types::Type};

use crate::{
    lifecycle::{LifecycleOutcome, LifecycleSnapshot},
    policy::StaticPolicy,
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
            INSERT INTO policies (name, phase, policy_json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?4)
            ON CONFLICT(name) DO UPDATE SET
                phase = excluded.phase,
                policy_json = excluded.policy_json,
                updated_at = excluded.updated_at
            "#,
            params![policy.name, phase, policy_json, now],
        )?;

        Ok(())
    }

    pub fn list_policies(&self) -> anyhow::Result<Vec<StaticPolicy>> {
        let connection = self.connection()?;
        let mut statement =
            connection.prepare("SELECT policy_json FROM policies ORDER BY name ASC")?;
        let policies = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .map(|row| {
                let policy_json = row?;
                serde_json::from_str::<StaticPolicy>(&policy_json)
                    .map_err(|error| json_decode_error(0, error))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(policies)
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

    fn migrate(&self) -> anyhow::Result<()> {
        self.connection()?.execute_batch(
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
            "#,
        )?;
        Ok(())
    }

    fn connection(&self) -> anyhow::Result<MutexGuard<'_, Connection>> {
        self.inner
            .lock()
            .map_err(|error| anyhow::anyhow!("storage lock poisoned: {error}"))
    }
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
        lifecycle::{LifecyclePhase, RequestLifecycle},
        policy::{PolicyPhase, StaticAction, StaticCondition, StaticEffect, StaticEffectKind},
    };

    use super::*;

    #[test]
    fn persists_policies_by_name() {
        let storage = Storage::in_memory().expect("storage");
        let policy = policy("require_tenant");

        storage.save_policy(&policy).expect("save policy");

        let policies = storage.list_policies().expect("list policies");
        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].name, "require_tenant");
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
