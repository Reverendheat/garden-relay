# Authentication and Multi-Tenancy

Garden Relay has two independent authentication paths:

- Relay clients authenticate apps with `X-Garden-Api-Key`.
- Administrators authenticate as operators and receive a server-side browser session.

The upstream provider credential remains in `Authorization` and is forwarded only to the configured provider.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `GARDEN_RELAY_AUTH_MODE` | `disabled` | Relay authentication rollout mode: `disabled`, `optional`, or `required`. |
| `GARDEN_RELAY_BOOTSTRAP_TOKEN` | unset | Creates and signs in the initial operator. Required to begin admin setup. |
| `GARDEN_RELAY_OPERATOR_SESSION_TTL_SECONDS` | `28800` | Operator session lifetime. Values below 60 seconds are raised to 60. |
| `GARDEN_RELAY_SESSION_COOKIE_SECURE` | `false` | Adds the `Secure` attribute to the operator cookie. Enable for HTTPS deployments. |

Production deployments must use a high-entropy bootstrap token, HTTPS, and `GARDEN_RELAY_SESSION_COOKIE_SECURE=true`. Remove the bootstrap token from the running environment after issuing a credential to a named operator.

## Bootstrap and Setup

1. Start the relay with `GARDEN_RELAY_BOOTSTRAP_TOKEN` configured.
2. Open `/ui` and sign in with the bootstrap token.
3. Create a tenant.
4. Create an app under the tenant.
5. Create any end users that the app may identify through `X-Garden-User`.
6. Create an app key and retain the one-time secret.
7. Create at least one named operator and retain its one-time login credential.
8. Remove `GARDEN_RELAY_BOOTSTRAP_TOKEN` from the production environment and restart.
9. Move `GARDEN_RELAY_AUTH_MODE` from `disabled` to `optional`, migrate clients, then use `required`.

## Relay Requests

```sh
curl http://127.0.0.1:8080/v1/chat/completions \
  -H "content-type: application/json" \
  -H "x-garden-api-key: $GARDEN_RELAY_API_KEY" \
  -H "authorization: Bearer $OPENAI_API_KEY" \
  -d '{
    "model": "gpt-4.1-mini",
    "messages": [{ "role": "user", "content": "Hello" }]
  }'
```

Tenant and app identity are derived from the relay key. Caller-provided tenant and app headers are replaced after successful authentication. Repeated failed verification attempts are rate-limited by key prefix.

## Operator API

Public endpoint:

```text
POST /v1/admin/login
```

Authenticated operator endpoints:

```text
POST   /v1/admin/logout
GET    /v1/admin/session
GET    /v1/tenants
POST   /v1/tenants
PATCH  /v1/tenants/{tenant_id}
GET    /v1/tenants/{tenant_id}/apps
POST   /v1/tenants/{tenant_id}/apps
GET    /v1/tenants/{tenant_id}/users
POST   /v1/tenants/{tenant_id}/users
GET    /v1/apps/{app_id}/keys
POST   /v1/apps/{app_id}/keys
DELETE /v1/apps/{app_id}/keys/{key_id}
GET    /v1/operators
POST   /v1/operators
DELETE /v1/operators/{operator_id}
GET    /v1/scoped-policies
POST   /v1/scoped-policies
```

Operator and app key creation responses contain their secret exactly once. Database records contain a visible prefix and an Argon2 password hash, never the plaintext secret. Deactivating an operator revokes its credentials and active sessions. An operator cannot deactivate its own account.

## Data Isolation

- Apps and users have tenant foreign keys and tenant-scoped uniqueness constraints.
- API keys are accepted only when both their app and tenant are active.
- Users supplied on relay requests must belong to the authenticated tenant.
- Policies may be global, tenant-scoped, or app-scoped and execute in that order.
- Lifecycle records store authenticated tenant, app, and user IDs and support tenant-constrained reads.
- Operator-only APIs are inaccessible without a valid, unexpired session cookie.

SQLite migrations preserve policies created before scoped policies existed by converting them to global policies with stable IDs.
