# Insighta Labs+ — CLI (insighta-cli)

## Overview

The CLI is a Rust binary that provides command-line access to the Insighta Labs+ API. It is installable globally via `cargo install --path .`, after which `insighta login` works from any directory.

The binary is structured across three files:
- `main.rs` — CLI argument parsing (clap derive)
- `api.rs` — All API interaction logic, PKCE generation, output formatting
- `config.rs` — Credentials storage at `~/.insighta/credentials.json`

---

## Authentication

### Login Flow (PKCE via Local Callback Server)

The `insighta login` command implements the full OAuth PKCE flow as specified in the TRD:

1. **Generate PKCE values locally**:
   - `state`: 32-char random alphanumeric string (for CSRF protection)
   - `code_verifier`: 64-char random string from `[A-Z][a-z][0-9]-._~` (PKCE secret)
   - `code_challenge`: Base64url-no-pad encoding of SHA-256 hash of `code_verifier`

2. **Start local callback server**: Binds `TcpListener` to `127.0.0.1:0` (random port), spawns a thread to accept a single HTTP connection

3. **Open browser**: Navigates to the backend's `/auth/github` endpoint with:
   - `redirect_url=http://localhost:<port>`
   - `state=<generated_state>`
   - `code_challenge=<derived_challenge>`
   - `code_challenge_method=S256`

4. **User authenticates on GitHub**: The backend redirects to GitHub, user approves

5. **GitHub redirects back**: Through the backend callback, which stores the GitHub auth code and redirects to the CLI's local server with `?code=<exchange_code>&state=<returned_state>`

6. **CLI captures callback**: The local server thread parses the HTTP request, extracts `code` and `state` parameters, sends them via channel, and returns a "Login successful" HTML page

7. **Validate state**: CLI compares `returned_state` against the `state` it generated. If they don't match, login is aborted (CSRF attack detection)

8. **Exchange code**: CLI sends `POST /auth/exchange-code` with `{ "code": "<exchange_code>", "code_verifier": "<verifier>" }`

9. **Backend completes exchange**: The backend uses the code_verifier to exchange the GitHub code for a GitHub access token, fetches user info, creates/updates the user, and returns JWT access + refresh tokens

10. **Fetch user info**: CLI calls `GET /auth/me` with the new access token to get username and role

11. **Store credentials**: Saved to `~/.insighta/credentials.json` with file permissions 0600 (owner read/write only on Unix)

12. **Confirm**: Prints "Logged in as @username"

### Why a Local Callback Server?

The TRD specifies that the CLI should start a temporary local callback server. This is necessary because:

- OAuth authorization codes are delivered via HTTP redirect
- The CLI needs to capture this redirect automatically (no manual copy-paste)
- The local server accepts one connection, extracts the code, and returns a success page
- This is the same pattern used by `gcloud auth login`, `gh auth login`, and similar CLIs

### Token Storage

Credentials are stored at `~/.insighta/credentials.json`:
```json
{
  "access_token": "eyJ...",
  "refresh_token": "019df5a7-...",
  "username": "davidifebueme",
  "role": "analyst",
  "api_url": "https://example.ngrok-free.app"
}
```

- File permissions: 0600 (only owner can read/write)
- The `role` is cached locally for UI purposes but always verified server-side
- The `api_url` is stored so the CLI knows which backend to talk to

### Token Lifecycle

- **Auto-refresh**: Before every authenticated API call, `auto_refresh()` checks if the current access token is still valid by calling `GET /auth/me`. If it returns 401, the CLI automatically calls `POST /auth/refresh` with the stored refresh token. If refresh succeeds, new tokens are saved. If refresh fails, credentials are cleared and the user is prompted to re-login.
- **All commands use auto-refresh**: `whoami`, `profiles list`, `profiles get`, `profiles search`, `profiles create`, `profiles export` — all go through `auto_refresh()` or `authenticated_get()`
- **On logout**: `POST /auth/logout` is called to revoke the refresh token server-side, then local credentials are deleted

---

## Commands

### Auth Commands

| Command | Description |
|---------|-------------|
| `insighta login` | Authenticate via GitHub OAuth with PKCE |
| `insighta logout` | Revoke refresh token and clear local credentials |
| `insighta whoami` | Display current user info (username, role, email, active status) |

### Profile Commands

| Command | Flags | Description |
|---------|-------|-------------|
| `insighta profiles list` | `--gender`, `--country`, `--age-group`, `--min-age`, `--max-age`, `--sort-by`, `--order`, `--page`, `--limit` | List profiles with filters, sorting, pagination |
| `insighta profiles get <id>` | — | Get single profile by ID |
| `insighta profiles search "<query>"` | `--page`, `--limit` | Natural language search |
| `insighta profiles create --name "Name"` | — | Create profile (admin only) |
| `insighta profiles export --format csv` | `--gender`, `--country` | Export profiles as CSV to current directory |

### API Version Header

All requests to `/api/*` endpoints include `X-API-Version: 1`. Auth endpoints (`/auth/*`) also include it for consistency.

---

## Output Formatting

- **Tables**: `comfy-table` with UTF8_FULL preset and rounded corners for list and search results
- **Spinners**: `indicatif` progress bars with 80ms steady tick for every operation (fetching, searching, creating, exporting)
- **Colors**: `console` crate for styled output (green for success, yellow for warnings, cyan for headers)
- **Errors**: Clear messages with actionable guidance ("Admin access required", "Session expired. Please run: insighta login")

---

## CSV Export

- Saves to the current working directory as `profiles_<YYYYMMDD_HHMMSS>.csv`
- Supports the same `--gender` and `--country` filters as list
- The file is written with `std::fs::write`

---

## Engineering Decisions

1. **Rust** — Consistent with the backend. Shares the same type safety and error handling philosophy. The binary is static-linked and has no runtime dependencies.

2. **PKCE generated locally** — The CLI generates `state`, `code_verifier`, and `code_challenge` itself rather than relying on the backend. This is the spec-compliant PKCE approach: the client proves it initiated the authorization request by presenting the `code_verifier` that corresponds to the `code_challenge` sent earlier. Even if the authorization code is intercepted, it can't be exchanged without the `code_verifier`.

3. **State validation** — The CLI validates the `state` parameter returned in the callback against the one it generated. This prevents CSRF attacks where an attacker could inject a pre-captured authorization code.

4. **Local callback server** — A raw `TcpListener` (not a full HTTP framework) binds to a random port, accepts exactly one connection, extracts query parameters from the raw HTTP request, and returns a minimal HTML response. This keeps the dependency footprint small.

5. **percent-encoding for URL params** — The `redirect_url`, `state`, and `code_challenge` are URL-encoded before being included in the auth URL, ensuring special characters (like `://` in the redirect URL) don't break the query string parsing.

6. **Auto-refresh before every call** — While slightly inefficient (it tests the token with `/auth/me` before each request), this approach is simple and reliable. The access token is only 3 minutes long, so refresh is frequent in practice.

7. **File permissions 0600** — Credentials are stored with restrictive permissions. On Unix, only the owner can read or write the file. This prevents other users on shared systems from reading tokens.

8. **Single binary, no config files** — The API URL comes from the `INSIGHTA_API_URL` environment variable (defaults to `http://localhost:3000`). Everything else is derived from the auth flow.
