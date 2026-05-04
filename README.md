# Insighta Web — Backend API

A demographic intelligence API built with Rust, Axum, SQLx, and PostgreSQL.

## Setup

1. Copy `.env.example` to `.env` and configure your database URL
2. Run migrations: `cargo sqlx migrate run`
3. Start the server: `cargo run`

The database auto-seeds with 2026 profiles on first startup.

## API Endpoints

### POST /api/profiles

Create a profile by name. Calls Genderize, Agify, and Nationalize APIs.

```json
{ "name": "ella" }
```

Returns 201 Created, or 200 with `"Profile already exists"` if the name already exists.

### GET /api/profiles

List profiles with filtering, sorting, and pagination.

**Filters:** `gender`, `country_id`, `age_group`, `min_age`, `max_age`, `min_gender_probability`, `min_country_probability`

**Sorting:** `sort_by` (age | created_at | gender_probability), `order` (asc | desc)

**Pagination:** `page` (default 1), `limit` (default 10, max 50)

Example: `/api/profiles?gender=male&country_id=NG&min_age=25&sort_by=age&order=desc&page=1&limit=10`

### GET /api/profiles/search

Natural language query endpoint with pagination (`page`, `limit`).

Example: `/api/profiles/search?q=young+males+from+nigeria`

### GET /api/profiles/{id}

Get a single profile by UUID.

### DELETE /api/profiles/{id}

Delete a profile. Returns 204 No Content.

## Natural Language Parsing Approach

The search endpoint uses a **rule-based keyword extraction parser** — no AI or LLMs.

### How it works

1. The query string is lowercased and tokenized by whitespace
2. Tokens are scanned left-to-right matching against known keyword patterns
3. Each match produces one or more filter constraints
4. All matched filters are combined with AND logic
5. If no meaningful tokens are matched, returns `"Unable to interpret query"`

### Supported keywords and mappings

| Keyword/Pattern | Maps to |
|---|---|
| `male`, `males` | `gender=male` |
| `female`, `females` | `gender=female` |
| `young` | `min_age=16`, `max_age=24` |
| `adult`, `adults` | `age_group=adult` |
| `teenager`, `teenagers` | `age_group=teenager` |
| `child`, `children` | `age_group=child` |
| `senior`, `seniors` | `age_group=senior` |
| `above X`, `over X`, `older X` | `min_age=X` |
| `below X`, `under X`, `younger X` | `max_age=X` |
| `from <country>` | `country_id=<code>` |
| Bare country name | `country_id=<code>` |

### Country name resolution

The parser maintains a lookup map of country names (and common aliases) to ISO codes. It supports:
- Full country names: "nigeria" → NG, "kenya" → KE
- Multi-word names: "south africa" → ZA, "dr congo" → CD
- Common aliases: "us"/"usa" → US, "uk"/"britain" → GB
- Partial name matching via word fragments

When `from` is detected, the parser greedily consumes subsequent non-keyword tokens as a country name. Without `from`, it still attempts to match bare country names as a fallback.

### Pagination

Search results support `page` and `limit` query parameters, defaulting to page 1 and limit 10.

## Limitations

- **No fuzzy matching**: Country names must closely match the stored lookup. "Niger" and "Nigeria" are different entries.
- **No boolean logic**: Cannot express OR conditions (e.g., "males or females from Kenya"). All filters are AND-combined.
- **No age ranges beyond "young"**: The only predefined age range is "young" (16-24). Arbitrary ranges like "ages 20 to 30" are not supported.
- **No negation**: Cannot express "not from Nigeria" or "excluding seniors".
- **"from" is greedy**: In "males from south africa over 30", "over" is consumed as part of the country name if not a keyword. Keywords are excluded from country name capture.
- **Gender keywords override**: If both "male" and "female" appear, the last one wins.
- **No stemming**: "teenage" won't match "teenager" — only exact keyword matches work.
- **Stop words are limited**: Only "and", "people", "persons", "person", "the" are ignored. Other non-keyword tokens may interfere with parsing.
