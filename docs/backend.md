# Insighta Labs+ — Backend (insighta-web)

## Overview

The backend is an Axum-based Rust web server that powers the Insighta Labs+ Profile Intelligence Platform. It exposes a REST API consumed by both the CLI and the web portal, handling authentication, authorization, profile data management, and natural language search.

The codebase follows a vertical slice architecture under `src/`:
- `auth/` — Authentication (OAuth, PKCE, JWT, refresh tokens)
- `profiles/` — Profile CRUD, filtering, sorting, pagination, NLP search, CSV export
- `shared/` — Error types, middleware (rate limiting, logging, API versioning), pagination, app state

---

## Authentication & Authorization

### GitHub OAuth Flow

The backend supports two distinct OAuth flows, both using GitHub as the identity provider with PKCE (Proof Key for Code Exchange):

#### Web Portal Flow (Server-Side PKCE)

1. Portal navigates user to `GET /auth/github?redirect_url=<portal_callback_url>`
2. Backend generates a random `state` and a PKCE pair (`code_verifier` + `code_challenge`)
3. Backend stores `{ state → { code_verifier, redirect_url, is_cli_flow: false } }` in an in-memory `PKCE_STORE`
4. Backend redirects to GitHub's OAuth authorize URL with `code_challenge` and `code_challenge_method=S256`
5. User authorizes on GitHub
6. GitHub redirects to `GET /auth/github/callback?code=XXX&state=YYY`
7. Backend looks up `state` in PKCE_STORE, retrieves the `code_verifier`
8. Backend exchanges `code` + `code_verifier` with GitHub's token endpoint
9. GitHub returns an access token; backend fetches user info from GitHub API
10. Backend upserts user in DB, issues JWT access token + opaque refresh token
11. Backend generates a one-time `auth_code`, stores it with the token pair in `AUTH_CODE_STORE`
12. Backend redirects to `redirect_url?code=<auth_code>`
13. Portal BFF calls `POST /auth/exchange-code` with `{ code: <auth_code }` to get tokens

#### CLI Flow (Client-Side PKCE)

1. CLI generates its own `state`, `code_verifier`, and `code_challenge` locally
2. CLI opens browser to `GET /auth/github?redirect_url=http://localhost:PORT&state=<state>&code_challenge=<challenge>&code_challenge_method=S256`
3. Backend sees the CLI-provided PKCE params, stores `{ state → { redirect_url, is_cli_flow: true } }` in PKCE_STORE (no code_verifier stored — CLI keeps it)
4. Backend redirects to GitHub with the CLI's `code_challenge`
5. User authorizes on GitHub
6. GitHub redirects to `GET /auth/github/callback?code=XXX&state=YYY`
7. Backend looks up `state`, sees `is_cli_flow: true`
8. Backend generates a one-time `exchange_code`, stores it with the GitHub auth code as `AuthCodeEntry::PendingGhCode(github_code)` in AUTH_CODE_STORE
9. Backend redirects to CLI's localhost server with `?code=<exchange_code>&state=<state>`
10. CLI validates that the returned `state` matches the one it generated (CSRF protection)
11. CLI calls `POST /auth/exchange-code` with `{ code: <exchange_code>, code_verifier: <verifier> }`
12. Backend retrieves the stored GitHub code, exchanges it + `code_verifier` with GitHub
13. Backend upserts user, issues tokens, returns them in the response body

### Token System

#### Access Token (JWT)

- **Format**: JSON Web Token (JWT) signed with HMAC-SHA256
- **Expiry**: 3 minutes (180 seconds)
- **Claims structure**:
  ```json
  {
    "sub": "<user UUID v7>",
    "role": "admin" | "analyst",
    "exp": 1714915200,
    "iat": 1714915020
  }
  ```
- **Transmission**:
  - CLI: Sent as `Authorization: Bearer <token>` header on every request
  - Portal: Stored in httpOnly cookie, read server-side by BFF proxy, forwarded as Bearer header
- **Validation**: Decoded and verified on every request using the `AuthUser` extractor in `auth/middleware.rs`

#### Refresh Token (Opaque)

- **Format**: UUID v7 (not a JWT — intentionally opaque to prevent information leakage)
- **Expiry**: 5 minutes (300 seconds)
- **Storage**: Hashed (SHA-256, base64url-no-pad) in the `refresh_tokens` database table
- **Transmission**:
  - CLI: Sent in `POST /auth/refresh` request body as `{"refresh_token": "..."}`
  - Portal: Stored in httpOnly cookie, read server-side by BFF proxy, sent in request body
- **Rotation**: On every refresh, the old token is deleted from DB and a new pair is issued
- **Revocation**: On logout, the token row is deleted from DB

#### Auth Code (One-Time Exchange Code)

- **Format**: UUID v7
- **Purpose**: Bridges the browser redirect (which can't carry secret tokens) with the token retrieval
- **Storage**: In-memory `AUTH_CODE_STORE` (HashMap behind Mutex)
- **Lifetime**: Single-use — consumed and removed on first exchange
- **Two variants**:
  - `AuthCodeEntry::Tokens(TokenPair)` — Web flow: tokens already issued, exchange just retrieves them
  - `AuthCodeEntry::PendingGhCode(String)` — CLI flow: GitHub code waiting for code_verifier to complete exchange

### Auth Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/auth/github` | Initiate GitHub OAuth (redirect or JSON) |
| GET | `/auth/github/callback` | Handle GitHub OAuth callback |
| POST | `/auth/exchange-code` | Exchange one-time auth code for tokens |
| POST | `/auth/refresh` | Refresh access token |
| POST | `/auth/logout` | Revoke refresh token |
| GET | `/auth/me` | Get current user info |

### Role-Based Access Control

- **admin**: Full access — can create, delete profiles, plus all read operations
- **analyst**: Read-only — can list, search, get, export profiles only
- Default role on new user creation: `analyst`
- Enforcement: The `AuthUser` extractor attaches `role` to every request. Profile handlers check `role != "admin"` for mutation endpoints and return 403 Forbidden.
- The `is_active` field on users gates all access — inactive users get 403 on every request.

### Users Table

| Column | Type | Notes |
|--------|------|-------|
| id | UUID v7 | Primary key, generated at application level |
| github_id | VARCHAR | Unique, stored as string for overflow safety |
| username | VARCHAR | GitHub username |
| email | VARCHAR | Nullable, from GitHub |
| avatar_url | VARCHAR | Nullable, GitHub avatar |
| role | VARCHAR | "admin" or "analyst", defaults to "analyst" |
| is_active | BOOLEAN | Defaults to true; false = 403 on all requests |
| last_login_at | TIMESTAMPTZ | Updated on every login |
| created_at | TIMESTAMPTZ | Defaults to NOW() |

---

## Profile APIs

### API Versioning

All `/api/*` endpoints require the `X-API-Version: 1` header. Requests without it receive:
```json
{ "status": "error", "message": "API version header required" }
```
Status: 400 Bad Request

### Endpoints

| Method | Path | Auth | Role | Description |
|--------|------|------|------|-------------|
| GET | `/api/profiles` | Required | Any | List profiles with filters, sorting, pagination |
| GET | `/api/profiles/search` | Required | Any | Natural language search |
| GET | `/api/profiles/:id` | Required | Any | Get single profile |
| POST | `/api/profiles` | Required | Admin | Create profile (calls external APIs) |
| DELETE | `/api/profiles/:id` | Required | Admin | Delete profile |
| GET | `/api/profiles/export` | Required | Any | Export profiles as CSV |

### Filtering Parameters (GET /api/profiles)

- `gender` — male, female
- `country_id` — 2-letter country code
- `age_group` — child, teen, adult, senior
- `min_age`, `max_age` — age range
- `sort_by` — name, age, gender, country
- `order` — asc, desc
- `page`, `limit` — pagination (default: page=1, limit=10)

### Pagination Response Format

```json
{
  "status": "success",
  "page": 1,
  "limit": 10,
  "total": 2026,
  "total_pages": 203,
  "links": {
    "self": "/api/profiles?page=1&limit=10",
    "next": "/api/profiles?page=2&limit=10",
    "prev": null
  },
  "data": [...]
}
```

### CSV Export

- `GET /api/profiles/export?format=csv` (supports same filters as list)
- Response: `Content-Type: text/csv`, `Content-Disposition: attachment; filename="profiles_<timestamp>.csv"`
- Column order: id, name, gender, gender_probability, age, age_group, country_id, country_name, country_probability, created_at

### Natural Language Search

The `GET /api/profiles/search?q=<query>` endpoint parses natural language queries into structured filters. The parser (`profiles/search.rs`) extracts:
- Gender keywords (male, female, man, woman, boy, girl)
- Age group keywords (child, teen, adult, senior, young, old, elderly)
- Age ranges ("25-40", "under 30", "over 50")
- Country names (mapped to country codes)
- Combinator logic (AND/OR)

---

## Middleware

### Rate Limiting

- Auth endpoints (`/auth/*`): 10 requests/minute per IP
- API endpoints (`/api/*`): 60 requests/minute per authenticated user
- Exceeded: Returns 429 Too Many Requests with `{ "status": "error", "message": "Rate limit exceeded" }`
- Implementation: In-memory HashMap tracking request timestamps per key

### Request Logging

Every request logs:
- HTTP method
- Endpoint (path)
- Status code
- Response time (milliseconds)

Uses `tracing::info!` with structured fields.

### API Version Middleware

Checks `X-API-Version` header on all `/api/*` paths. Rejects with 400 if missing or not "1".

---

## Error Responses

All errors follow a consistent format:
```json
{ "status": "error", "message": "Description of what went wrong" }
```

HTTP status codes: 400 (Bad Request), 401 (Unauthorized), 403 (Forbidden), 404 (Not Found), 422 (Unprocessable Entity), 429 (Too Many Requests), 500 (Internal Server Error), 502 (Bad Gateway)

---

## Engineering Decisions

1. **Rust/Axum** — Chosen for type safety, zero-cost abstractions, and async performance. Axum's tower-based middleware and extractor system made auth/versioning middleware clean.

2. **Opaque refresh tokens (UUID v7)** — Instead of JWT refresh tokens, we use opaque UUIDs. This prevents information leakage (JWTs are readable without the secret) and enables true server-side revocation (delete from DB). The hash stored in DB means even DB compromises don't reveal usable tokens.

3. **PKCE with dual flow** — The web portal flow generates PKCE server-side (simpler, tokens never touch the browser directly). The CLI flow generates PKCE client-side (required by spec — the CLI holds the code_verifier and only sends it during the final exchange, proving it initiated the request).

4. **One-time auth codes** — The callback redirect can't carry secret tokens in the URL. Instead, we issue a single-use code that the BFF/CLI exchanges server-to-server. This prevents tokens from appearing in browser history or referrer headers.

5. **In-memory PKCE/auth code stores** — Using `Lazy<Mutex<HashMap>>` for PKCE state and auth codes. These are short-lived (consumed within seconds) and single-use, so persistence isn't needed. This trades crash-recovery for simplicity and performance.

6. **Vertical slice architecture** — Each domain (auth, profiles, shared) is a self-contained module with its own handler, service, and model files. This keeps related code together and makes the codebase navigable.

7. **Runtime SQLx** — No compile-time query checking (which would require a live DB at build time). Queries are verified at runtime. This simplifies CI — no PostgreSQL service needed in GitHub Actions.

8. **CORS with specific origins** — Only the portal's origin (localhost:3001 for dev, Vercel URL for production) is allowed. `allow_credentials=true` enables cookie-based auth from the portal.

9. **github_id as VARCHAR** — GitHub IDs are i64 integers, but we store them as strings. This prevents overflow on 32-bit systems and makes the schema more resilient to future changes in GitHub's ID format.
