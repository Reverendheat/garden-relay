# Garden Relay

Garden Relay (get it, Guard and Relay?) is a policy-driven LLM gateway written in Rust.

## Local Docker Run

Build the image:

```sh
docker build -t gardenrelay:local .
```

Run the relay with a persistent local SQLite volume:

```sh
docker run --rm \
  -p 8080:8080 \
  -e OPENAI_BASE_URL=https://api.openai.com \
  -e GARDEN_RELAY_DATABASE_PATH=/data/gardenrelay.db \
  -e GARDEN_RELAY_LIFECYCLE_STORE_CAPACITY=1000 \
  -v gardenrelay-data:/data \
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
  -v gardenrelay-data:/data \
  gardenrelay:local
```

The Docker image stores SQLite data at `/data/gardenrelay.db` by default. When running the binary directly with `cargo run`, the default database path is `gardenrelay.db`.

The relay currently supports non-streaming `POST /v1/chat/completions`. Streaming requests return `501` until streaming proxy support is added.

## Request Timelines

Every relay request receives an `X-Garden-Request-Id` response header. Request lifecycles are persisted to SQLite and can be inspected locally:

```sh
curl http://127.0.0.1:8080/v1/requests
curl http://127.0.0.1:8080/v1/requests/{request_id}/timeline
```

They are stored at `GARDEN_RELAY_DATABASE_PATH`. A bounded in-memory cache still keeps the latest requests hot; change its size with `GARDEN_RELAY_LIFECYCLE_STORE_CAPACITY`.

## Local Admin UI

Open the built-in admin UI after starting the relay:

```text
http://127.0.0.1:8080/ui
```

The UI lists recent requests and active policies from the same local API endpoints used by clients.

The current UI is an MVP bundled into the relay for local iteration. Over time, Garden Relay will grow a separate UI service for richer administration workflows.

## Policies

On a fresh database, Garden Relay starts with no policies unless you opt into bootstrapping from `GARDEN_RELAY_POLICY_DIR`. Policies persisted in SQLite are loaded again on restart.

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

List active policies:

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

Policies added with `POST /v1/policies` are persisted to SQLite and are loaded again on restart.

## Human-in-the-loop workflows

Garden Relay does not own human approval or workflow resume loops. Policies can deny, log, disable tools, and mutate request or response content, but application and orchestration layers remain responsible for pausing work, collecting human input, and resuming workflow state.

## Roadmap

Planned areas:

| Area | Goal |
| --- | --- |
| Auth and multi-tenancy | Add relay-owned authentication, tenant/app/user management, scoped policy access, and supported admin workflows for managing tenants and operators. |
| More providers | Add first-class provider adapters beyond OpenAI-compatible chat completions, including Ollama, Anthropic, and Gemini. |
| PostgreSQL storage | Add a Postgres-backed storage provider for production and shared deployments while keeping SQLite as the simple local default. |
| Playground | Add a UI tab for testing policies against sample chat completion requests before rolling them into active traffic. |
| Separate UI service | Move beyond the bundled MVP UI toward a dedicated admin service for richer workflows and deployability. |
