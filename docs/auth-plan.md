# Authentication and Multi-Tenancy Plan

## Security Model

Garden Relay will distinguish these identities:

- **Tenant:** The primary isolation boundary.
- **App:** A client application belonging to a tenant.
- **User:** End-user attribution within a tenant.
- **Operator:** An administrator who manages tenants, apps, keys, users, and policies.
- **API key:** A credential that authenticates an app to the relay.
- **Operator session:** A browser session that authenticates an operator to the admin UI.

The initial request contract will keep relay and provider credentials separate:

```text
X-Garden-Api-Key: gr_live_...
Authorization: Bearer <provider-key>
X-Garden-User: user_123
```

Tenant and app identity must come from the validated relay API key. A caller must not be able to select or override either identity through `X-Garden-Tenant` or `X-Garden-App`.

## Implementation Steps

### 1. Persistent Identity Models

Add SQLite migrations and storage APIs for:

- `tenants`
- `apps`
- `users`
- `operators`
- `api_keys`
- `operator_sessions`

API key records must contain a stable ID, visible prefix, salted password hash, tenant and app scope, creation and expiration timestamps, revocation state, and last-used timestamp. Plaintext key secrets must never be persisted.

Change policies from globally keyed names to scoped records containing a stable policy ID, optional tenant and app IDs, name, and policy document. A policy with no tenant or app scope is global.

### 2. Authentication Layer

Create an Axum authentication module that:

1. Parses `X-Garden-Api-Key`.
2. Hashes and verifies the secret.
3. Rejects expired or revoked keys.
4. Loads the associated tenant and app.
5. adds an `AuthContext` to request extensions.
6. Validates that a supplied user belongs to the authenticated tenant.
7. Returns consistent `401` responses without revealing credential state.

Add rate limiting for failed authentication before the relay is exposed beyond local development.

### 3. Relay Request Integration

Update `POST /v1/chat/completions` so that:

- Tenant and app metadata come from `AuthContext`.
- Caller-provided tenant and app headers are ignored or rejected.
- Provider `Authorization` continues to be forwarded unchanged.
- Lifecycles record authenticated tenant, app, and user IDs.
- Authentication failures are recorded without credentials.
- A configurable migration mode can temporarily permit unauthenticated traffic.

Supported rollout modes:

```text
GARDEN_RELAY_AUTH_MODE=disabled
GARDEN_RELAY_AUTH_MODE=optional
GARDEN_RELAY_AUTH_MODE=required
```

### 4. Tenant Isolation

Apply authenticated scope to every relevant operation:

- Request lists and timelines
- Policy listing and mutation
- Playground evaluation
- Tenant, app, and user administration
- Future analytics and exports

Storage methods must require an explicit scope rather than returning all records for API handlers to filter.

Policy resolution must combine global, tenant, and app policies with deterministic ordering and documented name-collision behavior.

### 5. Management APIs

Add operator-protected endpoints for:

```text
POST   /v1/admin/login
POST   /v1/admin/logout
GET    /v1/admin/session

GET    /v1/tenants
POST   /v1/tenants
PATCH  /v1/tenants/{id}

GET    /v1/tenants/{id}/apps
POST   /v1/tenants/{id}/apps

GET    /v1/tenants/{id}/users
POST   /v1/tenants/{id}/users

POST   /v1/apps/{id}/keys
GET    /v1/apps/{id}/keys
DELETE /v1/apps/{id}/keys/{key_id}
```

A newly generated key is returned exactly once.

Bootstrap the first operator using an environment-provided token. Exchange that token for an `HttpOnly`, `Secure`, `SameSite=Strict` session cookie rather than retaining it in browser storage.

### 6. Admin UI

Add workflows for:

- Operator login and logout
- Tenant selection, listing, and creation
- App management within a tenant
- User management within a tenant
- API key creation, one-time reveal, listing, and revocation
- Policy scope selection
- Tenant-filtered request timelines

The playground must evaluate against an explicitly selected tenant and app scope instead of accepting arbitrary identity headers.

### 7. Verification

The implementation must test:

- Valid, invalid, expired, and revoked keys
- Key secrets are never persisted or logged
- Callers cannot override authenticated tenant or app identity
- Cross-tenant requests, policies, users, and timelines expose no data
- Users belong to the authenticated tenant
- Global, tenant, and app policy ordering
- Operator session expiration and logout invalidation
- Key rotation without downtime
- Separation and correct forwarding of provider credentials
- All authentication rollout modes

## Delivery Order

1. Identity schema and storage APIs.
2. Key generation, hashing, and `AuthContext`.
3. Chat-completion enforcement behind optional mode.
4. Tenant-scoped lifecycle and policy storage.
5. Management APIs.
6. Operator sessions and bootstrap flow.
7. Admin UI workflows.
8. Required-mode rollout, security review, and documentation.

The first implementation milestone ends after tenant-scoped lifecycle and policy storage. This establishes real tenant isolation before administration workflows depend on the storage contracts.

## Completion Criteria

The roadmap item is complete when relay clients and operators authenticate through separate supported mechanisms; tenant and app identities are derived from validated credentials; all tenant-owned data and policies are isolated in storage and APIs; management workflows exist for tenants, apps, users, operators, and keys; provider credentials remain separate; rollout modes and migration documentation exist; and the complete security test matrix passes.
