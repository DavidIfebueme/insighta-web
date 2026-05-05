# Insighta Labs+ — Web Portal (insighta-portal)

## Overview

The web portal is a Next.js application that provides a browser-based interface for the Insighta Labs+ platform. It targets non-technical users (analysts) who need to browse, search, and export profiles without using the CLI.

The portal does NOT call the backend API directly from the browser. Instead, it uses a **BFF (Backend-For-Frontend) proxy pattern** where all API calls go through Next.js server-side API routes. This keeps tokens in httpOnly cookies that are invisible to JavaScript.

---

## Authentication

### GitHub OAuth Flow (Web)

1. User clicks "Continue with GitHub" on `/login`
2. Browser navigates to `/api/auth/login?redirect_url=<encoded_callback_url>` (server-side route)
3. The BFF route redirects to the backend's `GET /auth/github?redirect_url=<portal_callback_url>`
4. Backend generates server-side PKCE, stores it, redirects to GitHub OAuth
5. User authorizes on GitHub
6. GitHub redirects to backend callback → backend exchanges code with GitHub → backend creates/updates user → backend issues tokens → backend generates one-time auth code → backend redirects to `portal/api/auth/callback?code=<auth_code>`
7. Portal BFF callback route calls `POST /auth/exchange-code` server-to-server with `{ code: <auth_code> }`
8. Backend returns `{ access_token, refresh_token }`
9. Portal BFF sets httpOnly cookies:
   - `access_token` — httpOnly, Secure (prod), SameSite=Lax, Max-Age=180 (3 min)
   - `refresh_token` — httpOnly, Secure (prod), SameSite=Lax, Max-Age=300 (5 min)
   - `csrf_token` — NOT httpOnly (readable by JS), Secure (prod), SameSite=Lax, Max-Age=300
10. Portal BFF redirects to `/dashboard`

### Token Storage & Transmission

Tokens are **never accessible to JavaScript**. Here's the full token flow:

| Step | Where | Mechanism |
|------|-------|-----------|
| Tokens stored | Browser cookies | httpOnly cookies set by server |
| API call initiated | Client JS | `fetch("/api/proxy/profiles")` — same-origin, browser auto-includes cookies |
| Cookies read | BFF server route | `request.cookies.get("access_token")` — server-side only |
| Token forwarded | BFF → Backend | `Authorization: Bearer <token>` header added by BFF |
| Token refreshed | BFF server route | On 401, BFF calls `/auth/refresh` with refresh_token cookie, re-sets both cookies |

The client-side JavaScript never sees raw token values. The `AuthProvider` context only receives user data (id, username, role, email, avatar_url) from `GET /api/auth/me`, never the tokens themselves.

### CSRF Protection

The portal uses the **double-submit cookie** pattern:

1. On login, the BFF callback sets a `csrf_token` cookie (NOT httpOnly — JavaScript can read it)
2. Client-side API library reads `csrf_token` from `document.cookie` and sends it as `X-CSRF-Token` header on all mutating requests (POST, DELETE)
3. The BFF proxy validates that the `X-CSRF-Token` header matches the `csrf_token` cookie on all POST/DELETE requests
4. If validation fails, returns 403 Forbidden

This works because:
- Same-origin requests include cookies automatically
- An attacker's site can't read the `csrf_token` cookie (same-origin policy)
- An attacker can't forge the `X-CSRF-Token` header without knowing the cookie value
- Combined with `SameSite=Lax`, this provides defense-in-depth

### Server-Side Auth Middleware

The `src/middleware.ts` file enforces authentication at the routing level:

- Public paths (`/login`, `/api/auth/*`, static assets) are allowed without auth
- All other paths require the `access_token` cookie
- If missing, the middleware redirects to `/login` server-side (before any HTML is rendered)
- This prevents the "flash of content" that would occur with client-side-only auth checks

---

## Pages

| Page | Route | Description |
|------|-------|-------------|
| Login | `/login` | GitHub OAuth button, error display |
| Dashboard | `/dashboard` | Total profiles count, recent profiles table |
| Profiles | `/profiles` | Filterable, sortable, paginated profiles list |
| Profile Detail | `/profiles/[id]` | Full profile info, delete button (admin only) |
| Search | `/search` | Natural language query input, results table |
| Account | `/account` | User info, role, permissions display |

### Role-Based UI

- **Admin**: Sees "Create Profile" button on profiles page, "Delete" button on profile detail
- **Analyst**: No create/delete buttons — read-only access enforced at the UI level
- Backend enforces the same rules at the API level (403 Forbidden for analysts attempting mutations)

---

## BFF Proxy Routes

### `/api/auth/login` (GET)

Constructs the backend OAuth URL server-side and redirects. Keeps the backend URL out of the client JavaScript bundle.

### `/api/auth/callback` (GET)

Handles the OAuth callback redirect from the backend. Exchanges the one-time auth code for tokens, sets httpOnly cookies + CSRF cookie, redirects to `/dashboard`.

### `/api/auth/me` (GET)

Proxies `GET /auth/me` to the backend. Reads access_token from cookie, adds Bearer header. Handles token refresh on 401.

### `/api/auth/logout` (POST)

Calls backend logout, clears all cookies (access_token, refresh_token, csrf_token).

### `/api/proxy/[...path]` (GET, POST, DELETE)

Catch-all proxy for all profile API calls:
- Reads `access_token` cookie → adds `Authorization: Bearer` header
- Adds `X-API-Version: 1` header
- Forwards query parameters (GET) or request body (POST)
- On 401: attempts token refresh using `refresh_token` cookie, retries the request, updates cookies
- On POST/DELETE: validates CSRF token (`X-CSRF-Token` header must match `csrf_token` cookie)
- Special handling for CSV responses (passes through as `text/csv` without JSON parsing)

---

## Engineering Decisions

1. **BFF Proxy Pattern** — The portal doesn't call the backend directly from the browser. All API calls go through Next.js API routes that act as a proxy. This is essential for three reasons:
   - **Token security**: Tokens live in httpOnly cookies that JavaScript can't read. The BFF reads cookies server-side and adds Bearer headers.
   - **CORS avoidance**: The backend's CORS policy only allows the portal's origin. The BFF makes server-to-server requests that bypass CORS entirely.
   - **Refresh handling**: When a token expires mid-session, the BFF can transparently refresh it and retry the request without the user noticing.

2. **httpOnly Cookies (not localStorage)** — The TRD explicitly requires HTTP-only cookies with tokens not accessible via JavaScript. Storing tokens in `localStorage` or `sessionStorage` would make them vulnerable to XSS attacks. httpOnly cookies are only accessible to the server.

3. **Secure flag conditional on environment** — The `Secure` cookie attribute is set to `true` only in production (`NODE_ENV === "production"`). In development (localhost, HTTP), browsers reject Secure cookies, so the portal would be non-functional. This is a common pattern.

4. **CSRF double-submit cookie** — Since the portal uses cookie-based auth, it's vulnerable to CSRF attacks (a malicious site could make requests that include the portal's cookies). The double-submit pattern requires the attacker to know the CSRF token, which they can't read from a different origin.

5. **Server-side middleware** — Next.js middleware checks for the `access_token` cookie before rendering protected pages. This prevents the "flash of unauthenticated content" where the page briefly renders before client-side JavaScript redirects to login.

6. **Backend URL kept server-side** — The login flow goes through `/api/auth/login` (a server route) rather than constructing the backend URL in the browser. This prevents the backend's internal URL from appearing in the client-side JavaScript bundle.

7. **SameSite=Lax** — All cookies use `SameSite=Lax`, which prevents them from being sent on cross-site POST/DELETE requests (the primary CSRF vector) while still allowing top-level navigations (needed for the OAuth redirect flow).

8. **Token expiry in cookies** — The cookie `maxAge` matches the actual token expiry (180s for access, 300s for refresh). This means the browser automatically deletes expired cookies, reducing the window for misuse.

9. **No SWR/React Query** — Data is fetched fresh on every page load using `useEffect` + `fetch`. No client-side caching layer. This ensures users always see real-time data from the backend.

10. **Next.js Image for avatars** — GitHub avatars are loaded using `next/image` with `remotePatterns` configured in `next.config.ts`. This enables automatic image optimization.
