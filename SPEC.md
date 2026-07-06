Garden Relay Spec

## Overview

Garden Relay is a policy-driven LLM gateway written in Rust.

It sits between applications and model providers, acting as a termination point for LLM traffic. Requests, responses, tool calls, routing decisions, retries, augmentation, redaction, and observability all pass through Garden Relay.

Garden Relay is not only a guardrail system. It is a programmable control plane for LLM traffic.

## Product Summary

Garden Relay gives teams control over:

* Where LLM requests go
* Which providers and models are used
* What context is added
* Which tools are allowed
* What gets blocked, rewritten, redacted, or retried
* How much requests cost
* Why each decision was made

Short description:

```text
Garden Relay is an OpenTelemetry-native LLM gateway for policy, routing, augmentation, and observability.
```

## Core Principle

Everything is a policy.

Routing, augmentation, retries, redaction, blocking, fallback, tool gating, eval sampling, and cost controls should all be expressible through policies.

```text
Policy = phase + condition/analyzer + effect
```

Examples:

```text
IF request contains a secret
THEN redact it before sending to the model
```

```text
IF tenant = "internal-tools" AND prompt category = "code"
THEN route to Claude Sonnet
```

```text
IF response has claims without citation IDs
THEN retry once with a citation repair instruction
```

```text
IF tool_call.name = "delete_file"
THEN deny or disable that tool before execution
```

## Enforcement Model

Garden Relay may use prompts to guide model behavior, but security-sensitive enforcement must be implemented through programmatic policies and gateway-owned controls.

The model can suggest actions.

Garden Relay decides whether those actions happen.

Examples of hard enforcement:

* Tool allowlists
* Tenant scoping
* File path restrictions
* Network restrictions
* Argument validation
* Budget limits
* Redaction
* Final allow/deny decisions

Bad enforcement:

```text
Tell the model: "Do not call delete_file unless the user confirms."
```

Good enforcement:

```rust
if tool_call.name == "delete_file" {
    return deny("delete_file is not allowed for this tenant");
}
```

Human approval and workflow resume belong to the application or orchestration layer, where graph state, business intent, approver identity, and resume semantics are available.

## Request Lifecycle

A normal request flows through these phases:

```text
request_received
  ↓
before_input
  ↓
before_model
  ↓
provider_call
  ↓
after_model
  ↓
before_response
  ↓
response_sent
```

Tool-using flows add:

```text
before_tool_call
after_tool_result
```

Retries and fallbacks may re-enter earlier phases.

## Policy Phases

### `request_received`

Runs when Garden Relay receives the request.

Useful for:

* Authentication
* Tenant lookup
* API key validation
* Request shape validation
* Rate limits
* Budget checks

### `before_input`

Runs before user input is accepted into the model request.

Useful for:

* Secret detection
* PII detection
* Prompt injection classification
* Input redaction
* Input normalization
* Request blocking

### `before_model`

Runs before calling the selected model provider.

Useful for:

* Provider routing
* Model selection
* Prompt augmentation
* RAG context insertion
* System/developer message insertion
* Tool availability changes
* Cost optimization
* Latency optimization
* Canary routing
* A/B testing

### `before_tool_call`

Runs before a model-proposed tool call is executed.

Useful for:

* Tool allowlists
* Tool denylists
* Argument validation
* Write/delete protection
* Network restrictions
* File path sandboxing
* Tenant ACL checks

### `after_tool_result`

Runs after a tool returns data, before that data is passed back to the model.

Useful for:

* Sanitizing tool output
* Redacting secrets
* Marking retrieved content as untrusted
* Removing embedded instructions
* Truncating large outputs
* Adding provenance metadata

### `after_model`

Runs after the model returns a response.

Useful for:

* Output validation
* Schema validation
* Citation checks
* Unsupported-claim detection
* Safety checks
* Retry/repair loops
* Response redaction

### `before_response`

Runs immediately before returning to the client.

Useful for:

* Final redaction
* Final block/allow decision
* Response formatting
* Metadata attachment

## Rust Runtime Model

All policies compile down to a common Rust runtime interface.

```rust
#[async_trait::async_trait]
pub trait Policy: Send + Sync {
    fn name(&self) -> &str;
    fn phase(&self) -> PolicyPhase;

    async fn evaluate(
        &self,
        ctx: &mut PolicyContext,
    ) -> anyhow::Result<Vec<PolicyEffect>>;
}
```

Garden Relay should support several policy implementations:

```text
StaticRulePolicy      UI-created IFTTT-style rules
ExpressionPolicy      rules using a safe expression language
AnalyzerPolicy        rules backed by structured analyzers
ClassifierPolicy      rules backed by local or remote classifiers
PluginPolicy          custom Rust or WASM policy plugins
```

The UI rule path and the programmatic plugin path should share the same runtime model.

```text
IFTTT is the UX.
Policies are the runtime.
Analyzers provide facts.
Effects change behavior.
```

## Policy Context

Policies operate on a shared context.

```rust
pub struct PolicyContext {
    pub request: RelayRequest,
    pub response: Option<ModelResponse>,
    pub tool_call: Option<ToolCall>,
    pub tool_result: Option<ToolResult>,
    pub route: Option<RouteDecision>,

    pub tenant: TenantContext,
    pub app: AppContext,
    pub auth: AuthContext,

    pub artifacts: PolicyArtifacts,
    pub effects: Vec<PolicyEffect>,
}
```

Policy artifacts expose facts produced by analyzers.

```rust
pub struct PolicyArtifacts {
    pub parsed_output: Option<serde_json::Value>,
    pub schema_valid: Option<bool>,

    pub classifications: HashMap<String, ClassificationResult>,
    pub redaction_spans: Vec<Span>,
    pub extracted_claims: Vec<Claim>,

    pub token_estimate: Option<TokenEstimate>,
    pub cost_estimate: Option<CostEstimate>,
}
```

This allows policies to evaluate structured data instead of raw text whenever possible.

## Programmatic IFTTT Rules

IFTTT-style rules should support programmatic analysis.

A UI-created rule may be simple:

```yaml
name: block_api_keys
phase: before_input

if:
  input_contains_secret: true

then:
  effect: deny
  reason: "Request contains a possible API key."
```

But a UI-created rule may also depend on analyzers:

```yaml
name: require_cited_claims
phase: after_model

analyze:
  - type: parse_json
    schema: cited_answer_v1
  - type: extract_claims

if:
  expr: "claims.exists(c, !c.citation_id)"

then:
  effect: retry
  instruction: "Every factual claim must include a citation_id. Remove unsupported claims."
  max_attempts: 1
```

Another example:

```yaml
name: protect_filesystem
phase: before_tool_call

if:
  expr: |
    tool.name == "write_file" &&
    !tool.args.path.starts_with(tenant.allowed_root)

then:
  effect: deny
  reason: "File path is outside the tenant root."
```

## Analyzers

Analyzers produce facts for policies.

Analyzers do not directly decide what happens. Policies consume analyzer results and produce effects.

Example analyzer types:

* Secret detector
* PII detector
* Prompt injection classifier
* Request classifier
* Tool intent classifier
* JSON schema validator
* Citation validator
* Claim extractor
* Unsupported claim checker
* Cost estimator
* Token estimator
* Latency tracker

Possible Rust trait:

```rust
#[async_trait::async_trait]
pub trait Analyzer: Send + Sync {
    fn name(&self) -> &str;
    fn phase(&self) -> PolicyPhase;

    async fn analyze(
        &self,
        ctx: &PolicyContext,
    ) -> anyhow::Result<AnalyzerOutput>;
}
```

Example classifier output:

```rust
pub struct ClassificationResult {
    pub label: String,
    pub score: f32,
    pub evidence: Vec<String>,
}
```

## Policy Effects

Policies produce effects.

```rust
pub enum PolicyEffect {
    Allow,

    Deny {
        reason: String,
    },

    Route {
        provider: String,
        model: String,
    },

    Augment {
        messages: Vec<Message>,
    },

    Rewrite {
        messages: Vec<Message>,
    },

    Redact {
        spans: Vec<Span>,
    },

    DisableTools {
        tools: Vec<String>,
    },

    Retry {
        instruction: String,
        max_attempts: Option<u32>,
    },

    Fallback {
        provider: String,
        model: String,
    },

    Cache {
        ttl_seconds: u64,
    },

    Log {
        level: LogLevel,
        message: String,
    },
}
```

The policy engine applies effects deterministically.

Blocking effects should generally win over permissive effects.

## Provider Routing

Garden Relay should support provider adapters.

Initial provider targets:

* OpenAI-compatible APIs
* Anthropic
* Ollama
* AWS Bedrock
* Local OpenAI-compatible endpoints

Routing policies may select providers based on:

* Tenant
* App
* User
* Request category
* Cost
* Latency
* Context window
* Tool support
* JSON/schema reliability
* Provider availability
* Canary percentage
* Fallback priority

Example:

```yaml
name: cheap_summaries_to_local
phase: before_model

if:
  category: summarization
  quality: standard

then:
  effect: route
  provider: ollama
  model: llama3.1
```

## Retry and Repair

Garden Relay owns retry behavior.

A model should not decide by itself whether policy validation passed.

Example flow:

```text
model response
  ↓
after_model policy fails
  ↓
Garden Relay applies retry effect
  ↓
model is called again with repair instruction
  ↓
after_model policies run again
  ↓
allow, deny, or escalate
```

Example policy:

```yaml
name: citation_repair
phase: after_model

if:
  missing_citations: true

then:
  effect: retry
  instruction: "Revise the answer using only cited source material. Remove unsupported claims."
  max_attempts: 1
```

## Tool Call Enforcement

All tool calls must pass through Garden Relay.

The model may propose a tool call, but Garden Relay decides whether it is allowed.

Tool policies should support:

* Tool allowlists
* Tool denylists
* Argument schema validation
* Path restrictions
* Network restrictions
* Tenant ACL checks
* Read/write/delete classification
* Budget checks

Example:

```yaml
name: block_destructive_tools
phase: before_tool_call

if:
  tool_intent: destructive

then:
  effect: deny
  reason: "Destructive tool calls are not allowed for this tenant."
```

## Observability

Observability is mandatory.

Garden Relay should emit OpenTelemetry-native traces, spans, events, metrics, and logs.

Every request must be traceable.

Every policy evaluation must be observable.

Every behavior-changing effect must be recorded.

Required invariant:

```text
No policy runs invisibly.
No provider call happens without a trace.
No retry, route, rewrite, deny, or redaction happens without an event.
```

Garden Relay should use OpenTelemetry as the observability substrate, while defining Garden-specific semantic attributes and events.

Example root span:

```text
garden.request
```

Useful attributes:

```text
garden.request_id
garden.tenant_id
garden.app_id
garden.user_id
garden.policy_count
garden.effects_applied
llm.provider
llm.model
llm.input_tokens
llm.output_tokens
llm.total_tokens
llm.cost_usd
```

Useful events:

```text
policy.evaluated
policy.matched
policy.effect.applied
policy.failed
llm.route.selected
llm.retry.started
llm.tool_call.denied
llm.output.redacted
```

## Web UI

The web UI should expose:

* Requests
* Traces
* Policies
* Providers
* Tenants/apps
* Costs
* Errors
* Retries
* Redactions
* Tool calls

The request detail view should show a timeline:

```text
[0ms] request_received
[4ms] before_input started
[7ms] policy block_api_keys: no match
[10ms] policy injection_classifier: matched, score=0.62
[12ms] effect disable_tools applied
[15ms] before_model started
[18ms] policy route_code_to_claude: matched
[20ms] route selected: anthropic/claude-sonnet
[1810ms] provider_call completed
[1820ms] after_model citation_check: failed
[1822ms] retry started
[3100ms] provider_call completed
[3120ms] response_sent
```

## API Shape

Garden Relay should initially expose an OpenAI-compatible endpoint.

```http
POST /v1/chat/completions
```

and/or:

```http
POST /v1/responses
```

Garden-native metadata may be passed through headers.

```text
X-Garden-Tenant: tenant_123
X-Garden-App: support_bot
X-Garden-User: user_456
```

A Garden-native endpoint may also exist:

```http
POST /v1/relay
```

Example request:

```json
{
  "tenant_id": "tenant_123",
  "app_id": "support_bot",
  "messages": [
    {
      "role": "user",
      "content": "Summarize this ticket."
    }
  ],
  "preferences": {
    "max_cost_usd": 0.02,
    "latency_priority": "medium"
  }
}
```

## Storage

Garden Relay should store:

* Tenants
* Apps
* Provider credentials
* Policies
* Policy versions
* Requests
* Trace metadata
* Policy decisions
* Cost records
* Eval samples

V0 can use SQLite or Postgres.

Policy versions must be recorded per request so Garden Relay can answer:

```text
Why did this request route to Claude yesterday but OpenAI today?
```

or:

```text
Which policy caused this response to be blocked?
```

## Security Principles

Garden Relay should enforce these principles:

* Treat user input as untrusted
* Treat retrieved documents as untrusted
* Treat tool output as untrusted
* Treat model output as untrusted until validated
* Do not rely on prompting alone for security-sensitive enforcement
* Enforce tenant boundaries outside the model
* Enforce tool permissions outside the model
* Store secrets securely
* Redact sensitive values from logs by default
* Preserve audit events for policy decisions

## MVP

The first MVP should include:

* Rust service
* OpenAI-compatible proxy endpoint
* One or two provider adapters
* Ordered policy phases
* Shared `Policy` trait
* Static UI-created rules
* Basic analyzer system
* Routing effect
* Augmentation effect
* Deny effect
* Retry effect
* Redaction effect
* OpenTelemetry traces
* Basic request timeline UI
* SQLite or Postgres storage
* Policy versioning

## Later Features

Potential future features:

* Classifier-backed policies
* LLM-as-judge policies
* CEL-style expression rules
* WASM policy plugins
* Rust plugin SDK
* Eval sampling
* Prompt/version experiments
* Canary routing
* Cost optimization
* Latency-aware routing
* Provider fallback
* Caching
* Tool marketplace
* Tenant-specific policy packs
* RAG context policies
* Compliance export
* Replay requests against new policies
* Shadow-mode policies
* Policy simulation before deployment

## Final Framing

Garden Relay is a Rust-based, OpenTelemetry-native LLM gateway.

It gives teams a programmable policy layer for routing, augmentation, enforcement, retries, provider control, and observability.

The important distinction:

```text
Prompts guide behavior.
Policies enforce behavior.
Analyzers produce facts.
Effects change execution.
OpenTelemetry explains what happened.
```
