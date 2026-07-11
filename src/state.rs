use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, MutexGuard},
};

use crate::{
    auth::{AuthMode, AuthService},
    config::Config,
    lifecycle::LifecycleSnapshot,
    policy::PolicyEngine,
    provider::openai_compatible::OpenAiCompatibleClient,
    storage::Storage,
};

#[derive(Clone)]
pub struct AppState {
    pub openai: OpenAiCompatibleClient,
    pub lifecycle_store: LifecycleStore,
    pub policy_engine: PolicyEngine,
    pub storage: Storage,
    pub auth: AuthService,
    pub auth_mode: AuthMode,
    pub session_cookie_secure: bool,
}

impl AppState {
    pub fn from_config(config: &Config) -> anyhow::Result<Self> {
        let storage = Storage::open(&config.database_path)?;
        let policy_engine = PolicyEngine::from_scoped_policies(storage.list_scoped_policies()?);

        for policy in PolicyEngine::from_dir(&config.policy_dir)?.list_policies() {
            storage.save_policy(&policy)?;
            policy_engine.add_policy(policy)?;
        }

        let mut state = Self::with_storage(
            OpenAiCompatibleClient::new(config.openai_base_url.clone()),
            LifecycleStore::new(config.lifecycle_store_capacity),
            policy_engine,
            storage,
        )
        .with_auth_mode(config.auth_mode);
        state.auth = state.auth.clone().with_operator_config(
            config.bootstrap_token.clone(),
            config.operator_session_ttl_seconds,
        );
        state.session_cookie_secure = config.session_cookie_secure;
        Ok(state)
    }

    #[cfg(test)]
    pub fn new(
        openai: OpenAiCompatibleClient,
        lifecycle_store: LifecycleStore,
        policy_engine: PolicyEngine,
    ) -> Self {
        Self::with_storage(
            openai,
            lifecycle_store,
            policy_engine,
            Storage::in_memory().expect("in-memory storage"),
        )
    }

    pub fn with_storage(
        openai: OpenAiCompatibleClient,
        lifecycle_store: LifecycleStore,
        policy_engine: PolicyEngine,
        storage: Storage,
    ) -> Self {
        let auth = AuthService::new(storage.clone());
        Self {
            openai,
            lifecycle_store,
            policy_engine,
            storage,
            auth,
            auth_mode: AuthMode::Disabled,
            session_cookie_secure: false,
        }
    }

    pub fn with_auth_mode(mut self, auth_mode: AuthMode) -> Self {
        self.auth_mode = auth_mode;
        self
    }

    pub fn save_lifecycle(&self, snapshot: LifecycleSnapshot) {
        self.lifecycle_store.save(snapshot.clone());
        if let Err(error) = self.storage.save_lifecycle(&snapshot) {
            tracing::error!("failed to persist lifecycle snapshot: {error}");
        }
    }
}

#[derive(Debug, Clone)]
pub struct LifecycleStore {
    inner: Arc<Mutex<LifecycleStoreInner>>,
    capacity: usize,
}

impl LifecycleStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(LifecycleStoreInner::default())),
            capacity: capacity.max(1),
        }
    }

    pub fn save(&self, snapshot: LifecycleSnapshot) {
        let Some(mut inner) = self.lock_inner() else {
            return;
        };

        if !inner.snapshots.contains_key(&snapshot.request_id) {
            inner.request_ids.push_back(snapshot.request_id.clone());
        }

        inner
            .snapshots
            .insert(snapshot.request_id.clone(), snapshot);

        while inner.snapshots.len() > self.capacity {
            let Some(request_id) = inner.request_ids.pop_front() else {
                break;
            };
            inner.snapshots.remove(&request_id);
        }
    }

    pub fn get(&self, request_id: &str) -> Option<LifecycleSnapshot> {
        self.lock_inner()?.snapshots.get(request_id).cloned()
    }

    pub fn list(&self) -> Vec<LifecycleSnapshot> {
        let Some(inner) = self.lock_inner() else {
            return Vec::new();
        };

        inner
            .request_ids
            .iter()
            .filter_map(|request_id| inner.snapshots.get(request_id).cloned())
            .collect()
    }

    fn lock_inner(&self) -> Option<MutexGuard<'_, LifecycleStoreInner>> {
        match self.inner.lock() {
            Ok(inner) => Some(inner),
            Err(error) => {
                tracing::error!("lifecycle store lock poisoned: {error}");
                None
            }
        }
    }
}

impl Default for LifecycleStore {
    fn default() -> Self {
        Self::new(1_000)
    }
}

#[derive(Debug, Default)]
struct LifecycleStoreInner {
    snapshots: HashMap<String, LifecycleSnapshot>,
    request_ids: VecDeque<String>,
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use crate::lifecycle::{LifecyclePhase, RequestLifecycle};

    use super::*;

    #[test]
    fn keeps_snapshots_in_insertion_order() {
        let store = LifecycleStore::new(10);
        let first = snapshot_with_id("first");
        let second = snapshot_with_id("second");

        store.save(first);
        store.save(second);

        let request_ids = store
            .list()
            .into_iter()
            .map(|snapshot| snapshot.request_id)
            .collect::<Vec<_>>();

        assert_eq!(request_ids, vec!["first", "second"]);
    }

    #[test]
    fn evicts_oldest_snapshot_when_capacity_is_reached() {
        let store = LifecycleStore::new(1);

        store.save(snapshot_with_id("first"));
        store.save(snapshot_with_id("second"));

        assert!(store.get("first").is_none());
        assert!(store.get("second").is_some());
    }

    fn snapshot_with_id(request_id: &str) -> LifecycleSnapshot {
        let mut lifecycle = RequestLifecycle::new();
        lifecycle.record_phase(LifecyclePhase::RequestReceived);
        lifecycle.record_success(StatusCode::OK);

        LifecycleSnapshot {
            request_id: request_id.to_owned(),
            ..lifecycle.snapshot()
        }
    }
}
