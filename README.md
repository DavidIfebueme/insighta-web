# insighta-web

backend api for insighta labs+ — a profile intelligence platform that collects data from genderize, agify, and nationalize apis, stores it, and serves it through a secure, multi-interface system.

## system architecture

```
[github oauth] → [backend (axum)] → [postgresql]
                       ↑
             ┌─────────┼─────────┐
             ↓         ↓         ↓
         [cli]    [web portal]  [api clients]
```

the backend is built with axum, sqlx, and postgresql. it handles auth, role-based access control, rate limiting, and serves profile data through a rest api. the cli and web portal both talk to the same backend — single source of truth.

**stack:** rust, axum, sqlx, postgresql, jsonwebtoken, reqwest

## auth flow

github oauth with pkce. two flows depending on the interface:

### cli flow (client-side pkce)

1. `insighta login` generates a `state`, `code_verifier`, and `code_challenge` locally
2. cli starts a temporary local callback server on a random port
3. cli opens the browser to `GET /auth/github?redirect_url=http://localhost:PORT&state=X&code_challenge=Y&code_challenge_method=S256`
4. backend stores the cli's code_challenge, passes it to github, redirects user to github oauth
5. user authenticates on github
6. github redirects to backend callback → backend stores the github code without exchanging (cli flow) → backend redirects to cli's localhost with `?code=EXCHANGE_CODE&state=X`
7. cli validates the returned state matches the one it generated (csrf protection)
8. cli sends `POST /auth/exchange-code` with `{ code: EXCHANGE_CODE, code_verifier: VERIFIER }`
9. backend exchanges the github code + code_verifier with github, upserts user, issues tokens
10. cli stores tokens at `~/.insighta/credentials.json` and prints "logged in as @username"

### web flow (server-side pkce)

1. user clicks "continue with github" on the portal
2. portal navigates to `/api/auth/login` (server route) → redirects to `GET /auth/github?redirect_url=PORTAL_CALLBACK_URL`
3. backend generates its own pkce pair, stores code_verifier with state, redirects to github
4. user authenticates on github
5. github redirects to backend callback → backend exchanges code + code_verifier with github → upserts user → issues tokens → generates one-time auth code → redirects to `PORTAL_CALLBACK_URL?code=AUTH_CODE`
6. portal bff calls `POST /auth/exchange-code` with `{ code: AUTH_CODE }` server-to-server
7. portal bff sets http-only cookies (access_token, refresh_token) + csrf_token cookie, redirects to dashboard

### token handling

- **access token**: 3 minutes. jwt signed with hmac-sha256, contains `sub` (user uuid v7), `role` ("admin"|"analyst"), `exp`, `iat`
- **refresh token**: 5 minutes. opaque uuid v7 (not a jwt — prevents info leakage). stored in db as sha256 hash. deleted on use — every refresh issues a new pair
- **on refresh**: old refresh token is deleted from db, new access + refresh pair is issued (`POST /auth/refresh`)
- **on logout**: refresh token is deleted from db, cookies are cleared
- **cli**: auto-refreshes tokens when they expire, falls back to re-login if refresh fails
- **web portal**: http-only, secure (prod), samesite=lax cookies — tokens never touch javascript. the bff proxy reads cookies server-side and adds `authorization: bearer` headers to backend requests. transparent token refresh on 401.

### one-time auth codes

the callback redirect can't carry secret tokens in the url. instead, the backend issues a single-use auth code that the bff/cli exchanges server-to-server. two variants:
- `authcodeentry::tokens(tokenpair)` — web flow: tokens already issued, exchange just retrieves them
- `authcodeentry::pendingghcode(string)` — cli flow: github code waiting for code_verifier to complete the exchange

## role enforcement

two roles: `admin` and `analyst` (default for new users)

| endpoint | admin | analyst |
|----------|-------|---------|
| `GET /api/profiles` | yes | yes |
| `GET /api/profiles/:id` | yes | yes |
| `GET /api/profiles/search` | yes | yes |
| `GET /api/profiles/export` | yes | yes |
| `POST /api/profiles` | yes | no (403) |
| `DELETE /api/profiles/:id` | yes | no (403) |

all `/api/*` endpoints require:
1. valid jwt in authorization header or http-only cookie
2. `x-api-version: 1` header
3. user must be active (`is_active = true`, otherwise 403)

role checks happen in the handler layer using the authuser extractor from the middleware. admin-only endpoints check `auth_user.role != "admin"` and return 403 forbidden.

## natural language parsing

the search endpoint (`GET /api/profiles/search?q=...`) parses natural language queries into structured filters. it looks for:

- **gender keywords:** "male", "female", "men", "women", "boys", "girls", etc.
- **age group keywords:** "young" → teenager, "old"/"elderly"/"senior" → senior, "child"/"children"/"kids" → child, "adult" → adult
- **age ranges:** "under 30" → max_age=30, "over 50" → min_age=50, "between 20 and 40" → min_age=20, max_age=40
- **country names:** maps 65+ country names and aliases to iso codes (e.g., "nigeria" → "ng", "uk" → "gb", "united states" → "us")

examples:
- "young males from nigeria" → gender=male, age_group=teenager, country_id=ng
- "senior females from kenya" → gender=female, age_group=senior, country_id=ke
- "adults over 40 from south africa" → age_group=adult, min_age=40, country_id=za

## api reference

### auth
- `GET /auth/github` — get github oauth url (with optional `redirect_url`, `state`, `code_challenge`, `code_challenge_method` params)
- `GET /auth/github/callback` — handle oauth callback
- `POST /auth/exchange-code` — exchange one-time auth code for tokens (accepts `code_verifier` for cli flow)
- `POST /auth/refresh` — refresh token pair
- `POST /auth/logout` — invalidate refresh token
- `GET /auth/me` — get current user info

### profiles
- `GET /api/profiles` — list profiles (paginated, filterable, sortable)
- `GET /api/profiles/:id` — get single profile
- `GET /api/profiles/search?q=...` — natural language search
- `GET /api/profiles/export?format=csv` — export to csv
- `POST /api/profiles` — create profile (admin only)
- `DELETE /api/profiles/:id` — delete profile (admin only)

### query params (list/export)
`gender`, `country_id`, `age_group`, `min_age`, `max_age`, `min_gender_probability`, `min_country_probability`, `sort_by` (age|gender_probability|created_at), `order` (asc|desc), `page`, `limit`

### pagination response
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

## rate limiting & logging

- auth endpoints (`/auth/*`): 10 requests/minute per ip
- api endpoints: 60 requests/minute per user
- returns 429 too many requests when exceeded

every request is logged with: method, endpoint, status code, response time (ms)

## setup

```bash
cp .env.example .env
# fill in your values
createdb insighta_web
sqlx migrate run
cargo run
```

## environment variables

| variable | description |
|----------|-------------|
| `DATABASE_URL` | postgres connection string |
| `DATABASE_MAX_CONNECTIONS` | pool size (default: 5) |
| `SERVER_ADDR` | listen address (default: 0.0.0.0:3000) |
| `GITHUB_CLIENT_ID` | github oauth app client id |
| `GITHUB_CLIENT_SECRET` | github oauth app client secret |
| `JWT_SECRET` | secret for signing jwts |
| `BASE_URL` | public url of this server |
