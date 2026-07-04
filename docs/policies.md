# Policy Authoring

Garden Relay starts with no active policies in a new database. Policies can be added at runtime with `POST /v1/policies` or bootstrapped from YAML files with `GARDEN_RELAY_POLICY_DIR`.

This document describes the policy fields supported by the current implementation.

## Shape

```json
{
  "name": "require_tenant",
  "phase": "before_model",
  "if": {
    "missing_header": "x-garden-tenant"
  },
  "then": {
    "effect": "deny",
    "reason": "Tenant header is required."
  }
}
```

The same shape is supported as YAML:

```yaml
name: require_tenant
phase: before_model

if:
  missing_header: x-garden-tenant

then:
  effect: deny
  reason: "Tenant header is required."
```

## Phases

Only these policy phases are currently implemented:

| Phase | Runs | Common use |
| --- | --- | --- |
| `before_input` | After the request body is parsed and normalized, before model routing/auth/provider work. | Block requests based on normalized request fields. |
| `before_model` | Immediately before provider auth and provider forwarding. | Enforce tenant/app headers, block specific models, or require routing prerequisites. |
| `after_model` | After the provider returns a response, before the relay returns it to the client. | Block responses based on provider output content or response JSON fields. |
| `before_response` | Immediately before returning the response to the client. | Final response checks. |

The broader spec includes more phases, but they are not implemented yet.

## Conditions

Conditions are ANDed together. If multiple condition fields are present, every condition must match.

| Field | Type | Match behavior |
| --- | --- | --- |
| `always` | boolean | `true` always matches. `false` never matches. |
| `missing_header` | string | Matches when the incoming HTTP request does not include that header. Header names are case-insensitive. |
| `header_equals` | object | Matches when a header exactly equals a value. Shape: `{ "name": "x-garden-app", "value": "support_bot" }`. |
| `model` | string | Matches when the normalized relay request model exactly equals this value. |
| `tenant_id` | string | Matches `X-Garden-Tenant` after header normalization. |
| `app_id` | string | Matches `X-Garden-App` after header normalization. |
| `user_id` | string | Matches `X-Garden-User` after header normalization. |
| `input_contains` | string | Matches when normalized message text contains this substring. |
| `request_json_equals` | object | Matches an exact value in the original request JSON. Shape: `{ "pointer": "/metadata/risk", "value": "high" }`. |
| `response_contains` | string | Matches when any string inside the provider response JSON contains this substring. Useful in `after_model` or `before_response`. |
| `response_json_equals` | object | Matches an exact value in the provider response JSON. Shape: `{ "pointer": "/choices/0/finish_reason", "value": "stop" }`. |
| `estimated_input_tokens_greater_than` | number | Matches when the relay's rough whitespace-based input token estimate is greater than this value. |
| `tool_name` | string | Matches when the original OpenAI-compatible request includes a tool with `function.name` equal to this value. |

At least one condition must be present.

Example requiring a tenant header:

```json
{
  "name": "require_tenant",
  "phase": "before_model",
  "if": { "missing_header": "x-garden-tenant" },
  "then": {
    "effect": "deny",
    "reason": "Tenant header is required."
  }
}
```

Example blocking one model:

```json
{
  "name": "block_expensive_model",
  "phase": "before_model",
  "if": { "model": "gpt-4.1" },
  "then": {
    "effect": "deny",
    "reason": "This model is not allowed for this relay."
  }
}
```

Example requiring a specific app and tenant:

```json
{
  "name": "internal_tools_only",
  "phase": "before_model",
  "if": {
    "tenant_id": "tenant_123",
    "app_id": "internal_tools"
  },
  "then": {
    "effect": "deny",
    "reason": "Internal tools are not allowed through this relay."
  }
}
```

Example blocking prompt content:

```json
{
  "name": "block_secret_requests",
  "phase": "before_input",
  "if": { "input_contains": "BEGIN PRIVATE KEY" },
  "then": {
    "effect": "deny",
    "reason": "Request appears to contain a private key."
  }
}
```

Example blocking a tool:

```json
{
  "name": "block_delete_file",
  "phase": "before_model",
  "if": { "tool_name": "delete_file" },
  "then": {
    "effect": "deny",
    "reason": "delete_file is not allowed."
  }
}
```

Example blocking a provider response:

```json
{
  "name": "block_unsupported_claims",
  "phase": "after_model",
  "if": { "response_contains": "unsupported claim" },
  "then": {
    "effect": "deny",
    "reason": "Response contained unsupported claim text."
  }
}
```

JSON field conditions use [JSON Pointer](https://datatracker.ietf.org/doc/html/rfc6901) syntax. For example, `/messages/0/content` addresses `request.messages[0].content`.

## Effects

Policies can define one effect:

```json
"then": {
  "effect": "deny",
  "reason": "Tenant header is required."
}
```

Or multiple effects:

```json
"then": {
  "effects": [
    {
      "effect": "log",
      "level": "info",
      "message": "Tenant policy matched."
    },
    {
      "effect": "disable_tools",
      "tools": ["delete_file"]
    }
  ]
}
```

Currently implemented effects:

| Effect | Fields | Behavior |
| --- | --- | --- |
| `deny` | `reason` optional string | Stops the request and returns `403 policy_denied`. |
| `log` | `level` optional `trace`/`debug`/`info`/`warn`/`error`, `message` optional string | Records a lifecycle policy effect event and emits a tracing log. Does not stop the request. |
| `disable_tools` | `tools` array of tool names | Removes matching OpenAI-compatible tools from the outgoing request before provider forwarding. Only applies in request-side phases. |
| `augment` | `messages` array of `{ "role": "...", "content": "..." }` | Appends messages to the outgoing OpenAI-compatible request before provider forwarding. Only applies in request-side phases. |
| `require_approval` | `reason` optional string | Stops the request, returns `409 approval_required`, and creates a pending approval record. Approved requests can continue when retried with `X-Garden-Approval-Id`. |

If a terminal effect such as `deny` or `require_approval` fires, provider forwarding does not happen. The request timeline records:

```text
policy.evaluated
policy.effect.applied
approval.created
request_failed
```

`approval.created` only appears for `require_approval`.

After a human approves the request through the UI or API, retry the same chat completion request with:

```text
X-Garden-Approval-Id: {approval_id}
```

Garden Relay verifies that the approval exists, is approved, matches the policy requiring approval, and matches the retried request body. The approval is consumed after use.

Example requiring approval for a tool:

```json
{
  "name": "approval_for_delete_file",
  "phase": "before_model",
  "if": { "tool_name": "delete_file" },
  "then": {
    "effect": "require_approval",
    "reason": "delete_file requires human approval."
  }
}
```

Example augmenting a request:

```json
{
  "name": "add_support_bot_instruction",
  "phase": "before_model",
  "if": { "app_id": "support_bot" },
  "then": {
    "effect": "augment",
    "messages": [
      {
        "role": "system",
        "content": "Use a concise support tone."
      }
    ]
  }
}
```

## Runtime API

Add or replace a policy by name:

```sh
curl http://127.0.0.1:8080/v1/policies \
  -H "content-type: application/json" \
  -d '{
    "name": "require_tenant",
    "phase": "before_model",
    "if": { "missing_header": "x-garden-tenant" },
    "then": {
      "effect": "deny",
      "reason": "Tenant header is required."
    }
  }'
```

List active policies:

```sh
curl http://127.0.0.1:8080/v1/policies
```

Policies added through this endpoint are persisted to SQLite at `GARDEN_RELAY_DATABASE_PATH`.

List and decide approvals:

```sh
curl http://127.0.0.1:8080/v1/approvals
curl http://127.0.0.1:8080/v1/approvals/{approval_id}
curl -X POST http://127.0.0.1:8080/v1/approvals/{approval_id}/approve
curl -X POST http://127.0.0.1:8080/v1/approvals/{approval_id}/deny
```

## File Bootstrap

Runtime API is the intended local UX. File bootstrap is still available for startup defaults:

```sh
GARDEN_RELAY_POLICY_DIR=examples/policies cargo run
```

Garden Relay loads every `*.yaml` and `*.yml` file in that directory at startup. Runtime-added policies are persisted to SQLite.

## Not Implemented Yet

These are planned but not part of the current policy engine:

| Capability | Status |
| --- | --- |
| Cost estimates | Not implemented. |
| Analyzer outputs | Not implemented. |
| Expression language such as `tenant == "internal" && model.starts_with("gpt-")` | Not implemented. |
| Automatic background replay after approval | Not implemented. Retry with `X-Garden-Approval-Id` instead. |
| Route, retry, redact, or prompt-user effects | Not implemented. |
