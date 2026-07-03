use std::time::Instant;

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::domain::RelayRequest;

#[derive(Debug, Clone)]
pub struct RequestLifecycle {
    request_id: String,
    started_at: Instant,
    pub current_phase: LifecyclePhase,
    pub relay_request: Option<RelayRequest>,
    pub events: Vec<LifecycleEvent>,
    pub outcome: Option<LifecycleOutcome>,
}

impl RequestLifecycle {
    pub fn new() -> Self {
        Self {
            request_id: Uuid::new_v4().to_string(),
            started_at: Instant::now(),
            current_phase: LifecyclePhase::RequestReceived,
            relay_request: None,
            events: Vec::new(),
            outcome: None,
        }
    }

    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub fn snapshot(&self) -> LifecycleSnapshot {
        LifecycleSnapshot {
            request_id: self.request_id.clone(),
            current_phase: self.current_phase,
            relay_request: self.relay_request.clone(),
            events: self.events.clone(),
            outcome: self.outcome.clone(),
        }
    }

    pub fn set_relay_request(&mut self, request: RelayRequest) {
        self.relay_request = Some(request);
    }

    pub fn record_phase(&mut self, phase: LifecyclePhase) {
        self.current_phase = phase;
        self.record_event("phase_started", json!({}));
    }

    pub fn record_event(&mut self, name: impl Into<String>, details: Value) {
        self.events.push(LifecycleEvent {
            phase: self.current_phase,
            name: name.into(),
            elapsed_ms: self.started_at.elapsed().as_millis(),
            details,
        });
    }

    pub fn record_success(&mut self, status: StatusCode) {
        self.outcome = Some(LifecycleOutcome::Completed {
            status_code: status.as_u16(),
        });
        self.record_event(
            "request_completed",
            json!({ "status_code": status.as_u16() }),
        );
    }

    pub fn record_failure(&mut self, status: StatusCode, code: &'static str) {
        self.outcome = Some(LifecycleOutcome::Failed {
            status_code: status.as_u16(),
            code: code.to_owned(),
        });
        self.record_event(
            "request_failed",
            json!({ "status_code": status.as_u16(), "code": code }),
        );
    }

    pub fn emit_tracing_events(&self) {
        for event in &self.events {
            tracing::info!(
                garden.request_id = %self.request_id,
                garden.phase = event.phase.as_str(),
                garden.event = %event.name,
                garden.elapsed_ms = event.elapsed_ms,
                garden.details = %event.details,
                "garden lifecycle event"
            );
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleSnapshot {
    pub request_id: String,
    pub current_phase: LifecyclePhase,
    pub relay_request: Option<RelayRequest>,
    pub events: Vec<LifecycleEvent>,
    pub outcome: Option<LifecycleOutcome>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecyclePhase {
    RequestReceived,
    BeforeInput,
    BeforeModel,
    ProviderCall,
    AfterModel,
    BeforeResponse,
    ResponseSent,
}

impl LifecyclePhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RequestReceived => "request_received",
            Self::BeforeInput => "before_input",
            Self::BeforeModel => "before_model",
            Self::ProviderCall => "provider_call",
            Self::AfterModel => "after_model",
            Self::BeforeResponse => "before_response",
            Self::ResponseSent => "response_sent",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleEvent {
    pub phase: LifecyclePhase,
    pub name: String,
    pub elapsed_ms: u128,
    pub details: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LifecycleOutcome {
    Completed { status_code: u16 },
    Failed { status_code: u16, code: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_phase_events_in_order() {
        let mut lifecycle = RequestLifecycle::new();

        lifecycle.record_phase(LifecyclePhase::RequestReceived);
        lifecycle.record_phase(LifecyclePhase::BeforeInput);
        lifecycle.record_success(StatusCode::OK);

        assert_eq!(lifecycle.events[0].phase, LifecyclePhase::RequestReceived);
        assert_eq!(lifecycle.events[1].phase, LifecyclePhase::BeforeInput);
        assert!(matches!(
            lifecycle.outcome,
            Some(LifecycleOutcome::Completed { status_code: 200 })
        ));
    }

    #[test]
    fn snapshots_keep_request_timeline() {
        let mut lifecycle = RequestLifecycle::new();

        lifecycle.record_phase(LifecyclePhase::RequestReceived);
        lifecycle.record_failure(StatusCode::UNAUTHORIZED, "authentication_error");

        let snapshot = lifecycle.snapshot();

        assert_eq!(snapshot.request_id, lifecycle.request_id());
        assert_eq!(snapshot.events.len(), 2);
        assert!(matches!(
            snapshot.outcome,
            Some(LifecycleOutcome::Failed {
                status_code: 401,
                ref code,
            })
            if code == "authentication_error"
        ));
    }
}
