# Garden Relay

Garden Relay (get it, Guard and Relay?) is a policy-driven LLM gateway written in Rust.

## Local Development

For local development with [just](https://just.systems/), start the relay with:

```sh
just run
```

The optional arguments are the port and SQLite database path:

```sh
just run 18080 /tmp/gardenrelay.db
```

Use `just run-with-policies` to bootstrap the policies under `examples/policies`, or `just check` to run formatting checks, Clippy, and tests.

The local admin UI login token defaults to `garden-local-admin`. Override it with `GARDEN_RELAY_BOOTSTRAP_TOKEN`.

Without `just`, the equivalent default command is `cargo run`.

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
  -e GARDEN_RELAY_BOOTSTRAP_TOKEN="$GARDEN_RELAY_BOOTSTRAP_TOKEN" \
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

Sign in using `GARDEN_RELAY_BOOTSTRAP_TOKEN`. The token creates the first operator on its first successful use and is exchanged for an `HttpOnly`, `SameSite=Strict` server-side session. Set `GARDEN_RELAY_SESSION_COOKIE_SECURE=true` when the UI is served over HTTPS.

The current UI is an MVP bundled into the relay for local iteration. Over time, Garden Relay will grow a separate UI service for richer administration workflows.

### Policy Playground

The **Playground** tab evaluates a draft policy against a sample OpenAI-compatible chat completion without saving the policy, calling a provider, or adding a request timeline. Supply sample Garden headers to test tenant, app, user, and header conditions. An optional provider response supports `after_model` and `before_response` conditions.

The same simulation is available over HTTP:

```sh
curl http://127.0.0.1:8080/v1/playground/evaluate \
  -H "content-type: application/json" \
  -d '{
    "policy": {
      "name": "require_tenant",
      "phase": "before_model",
      "if": { "missing_header": "x-garden-tenant" },
      "then": { "effect": "deny", "reason": "Tenant header is required." }
    },
    "headers": {},
    "request": {
      "model": "gpt-4.1-mini",
      "messages": [{ "role": "user", "content": "Hello" }]
    }
  }'
```

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

## Authentication and Multi-Tenancy

Relay authentication is separate from provider authentication:

```text
X-Garden-Api-Key: gr_live_...
Authorization: Bearer <provider-key>
X-Garden-User: user_123
```

The relay key determines the tenant and app. Authenticated callers cannot override them with `X-Garden-Tenant` or `X-Garden-App`. If `X-Garden-User` is supplied, it must identify an active user in the authenticated tenant. Provider authorization continues to be forwarded without being persisted.

Configure rollout behavior with `GARDEN_RELAY_AUTH_MODE`:

| Mode | Behavior |
| --- | --- |
| `disabled` | Relay keys are not evaluated and legacy Garden identity headers remain available. This is the default migration mode. |
| `optional` | Requests without a relay key are accepted; supplied keys must be valid. |
| `required` | Every chat completion requires a valid relay key. |

The **Tenants** admin tab manages tenants, apps, users, relay keys, and operators. Relay and operator secrets are displayed once and stored only as Argon2 password hashes. Revoking a relay key takes effect immediately; creating a replacement before revoking the old key provides rotation without downtime.

Operator management APIs, scoped policies, request timelines, and the playground require an authenticated operator session. Policies execute in deterministic global, tenant, then app order. Request timelines can be filtered by tenant, and playground identity comes from the selected tenant/app scope.

See [docs/authentication.md](docs/authentication.md) for configuration, endpoints, deployment requirements, and migration steps.

## Human-in-the-loop workflows

Garden Relay does not own human approval or workflow resume loops. Policies can deny, log, disable tools, and mutate request or response content, but application and orchestration layers remain responsible for pausing work, collecting human input, and resuming workflow state.

## Roadmap

Planned areas:

| Area | Goal |
| --- | --- |
| More providers | Add first-class provider adapters beyond OpenAI-compatible chat completions, including Ollama, Anthropic, and Gemini. |
| PostgreSQL storage | Add a Postgres-backed storage provider for production and shared deployments while keeping SQLite as the simple local default. |
| Separate UI service | Move beyond the bundled MVP UI toward a dedicated admin service for richer workflows and deployability. |
