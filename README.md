# Garden Relay

Garden Relay is a policy-driven LLM gateway written in Rust.

## Local Docker Run

Build the image:

```sh
docker build -t gardenrelay:local .
```

Run the relay:

```sh
docker run --rm \
  -p 8080:8080 \
  -e OPENAI_BASE_URL=https://api.openai.com \
  -e GARDEN_RELAY_LIFECYCLE_STORE_CAPACITY=1000 \
  gardenrelay:local
```

Garden Relay does not need an OpenAI API key in its environment for this mode. Pass provider credentials through on each request:

```sh
curl http://127.0.0.1:8080/v1/chat/completions \
  -H "content-type: application/json" \
  -H "authorization: Bearer $OPENAI_API_KEY" \
  -H "x-garden-tenant: tenant_123" \
  -d '{
    "model": "gpt-4.1-mini",
    "messages": [
      { "role": "user", "content": "Say hello from behind Garden Relay." }
    ]
  }'
```

For a local OpenAI-compatible provider, point `OPENAI_BASE_URL` at that server:

```sh
docker run --rm \
  -p 8080:8080 \
  -e OPENAI_BASE_URL=http://host.docker.internal:11434 \
  gardenrelay:local
```

The relay currently supports non-streaming `POST /v1/chat/completions`. Streaming requests return `501` until streaming proxy support is added.

## Request Timelines

Every relay request receives an `X-Garden-Request-Id` response header. Recent request lifecycles are kept in a bounded in-memory store and can be inspected locally:

```sh
curl http://127.0.0.1:8080/v1/requests
curl http://127.0.0.1:8080/v1/requests/{request_id}/timeline
```

The in-memory store defaults to the latest 1,000 requests. Change it with `GARDEN_RELAY_LIFECYCLE_STORE_CAPACITY`.

## Policies

Garden Relay starts with no policies unless you opt into bootstrapping from `GARDEN_RELAY_POLICY_DIR`. If the directory does not exist, no policies are loaded.

Supported phases, conditions, effects, and examples are documented in [docs/policies.md](docs/policies.md).

Add a policy while the service is running:

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

List active in-memory policies:

```sh
curl http://127.0.0.1:8080/v1/policies
```

You can also bootstrap policies from files at startup:

```sh
mkdir -p policies
cp examples/policies/require_tenant.yaml policies/
GARDEN_RELAY_POLICY_DIR=policies cargo run
```

Then send a request without `x-garden-tenant`; the relay will deny it before provider forwarding and record the policy decision in the request timeline.
