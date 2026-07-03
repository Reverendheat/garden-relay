# Policy Authoring

Garden Relay starts with no active policies. Policies can be added at runtime with `POST /v1/policies` or bootstrapped from YAML files with `GARDEN_RELAY_POLICY_DIR`.

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

The broader spec includes more phases, but they are not implemented yet.

## Conditions

Conditions are ANDed together. If multiple condition fields are present, every condition must match.

| Field | Type | Match behavior |
| --- | --- | --- |
| `always` | boolean | `true` always matches. `false` never matches. |
| `missing_header` | string | Matches when the incoming HTTP request does not include that header. Header names are case-insensitive. |
| `model` | string | Matches when the normalized relay request model exactly equals this value. |

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

## Effects

Only this effect is currently implemented:

| Effect | Fields | Behavior |
| --- | --- | --- |
| `deny` | `reason` optional string | Stops the request and returns `403 policy_denied`. |

If a `deny` effect fires, provider forwarding does not happen. The request timeline records:

```text
policy.evaluated
policy.effect.applied
request_failed
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

## File Bootstrap

Runtime API is the intended local UX. File bootstrap is still available for startup defaults:

```sh
GARDEN_RELAY_POLICY_DIR=examples/policies cargo run
```

Garden Relay loads every `*.yaml` and `*.yml` file in that directory at startup. Runtime-added policies are currently in-memory only.
