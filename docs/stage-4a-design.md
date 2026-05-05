# scaling insighta labs+: system design under growth

## context

insighta labs+ is a demographic intelligence platform. users submit queries — structured filters and rule-based keyword searches — the system processes them against a postgresql-backed store of profile records, and returns results. the platform is accessed through a rest api, a rust cli, and a next.js web portal. github oauth + rbac governs who can do what.

this document is not about new features. it's about what happens when the working system we already built actually gets used at scale.

---

## 1. requirements

### functional

the system must continue to do everything it does today — no regressions from stage 3:
- github oauth authentication (pkce for cli, server-side for web)
- role-based access control (admin can write, analyst is read-only)
- profile filtering (gender, country, age group, age range, probability thresholds)
- profile sorting (by age, gender_probability, created_at)
- offset-based pagination with total count and hateoas links
- natural language search (rule-based keyword parser mapping to structured filters)
- csv export with the same filter support
- rate limiting and request logging
- cli and web portal both hitting the same backend

### non-functional

| requirement | target | why |
|-------------|--------|-----|
| p50 query latency | < 500ms | interactive feel for analysts browsing profiles |
| p95 query latency | < 2s | acceptable worst-case for complex filtered queries |
| dataset scale | 10–30m profiles | stated growth target |
| query throughput | 100–1000 qpm | hundreds of concurrent users across teams |
| availability | 99.9% | internal tool, but relied upon daily — ~8.7h downtime/year max |
| consistency | eventual acceptable for reads | demographic data is not financial — a few minutes of staleness is fine |
| single-region | us east | no geographic distribution requirement |

---

## 2. architecture

### current state

```
[cli]  [web portal]
   \       /
    \     /
  [axum backend]
       |
  [postgresql — single instance]
```

right now everything runs on one box. one axum process, one postgres instance, no caching, no replicas. the profiles table has a single index on `lower(name)`. every query hits the database directly. this works fine at 2k profiles. it will not work at 20 million.

### proposed architecture

```
                            ┌──────────────┐
                            │  cloudflare  │
                            │  (cdn/dns)   │
                            └──────┬───────┘
                                   │
                     ┌─────────────┼─────────────┐
                     ↓             ↓             ↓
                 [cli]      [web portal]    [api clients]
                               \       /
                                \     /
                            ┌────────┴────────┐
                            │  axum backend    │
                            │  ┌────────────┐  │
                            │  │ moka cache │  │
                            │  └────────────┘  │
                            └────┬────────┬────┘
                                 │        │
                    ┌────────────┘        └────────────┐
                    ↓                                  ↓
           ┌──────────────┐                    ┌──────────────┐
           │  pg primary  │──── streaming ────→│  pg replica  │
           │  (writes +   │     replication    │  (all reads) │
           │   critical)  │                    │              │
           └──────────────┘                    └──────────────┘
```

**what changed and why:**

1. **moka cache** (in-process) — our queries are read-heavy and ~40% are repeated. an in-process cache avoids a network hop and has zero operational overhead. we start here, add redis only if we run multiple api instances.

2. **postgresql read replica** — the single highest-roi change. route all read queries (which is 95%+ of our traffic) to a streaming replica. this doubles read capacity with zero application logic changes — just two pool handles in `appstate` instead of one.

3. **cloudflare in front** — not for caching (our endpoints require auth), but for ddos protection, ssl termination, and keeping ngrok out of the picture long-term.

what we did NOT add and why:
- **no message queue** — our writes are synchronous (profile creation calls external apis) or batch. there's no async work to decouple.
- **no microservices** — we have one bounded context (profiles + auth). splitting it adds network calls and operational complexity for zero benefit.
- **no separate search engine** (yet) — our nlp parser maps to structured filters that postgresql can serve well with proper indexes. we'd only add elasticsearch if postgres can't meet the 2s p95 despite optimization.

---

## 3. data flow

### query path (the hot path)

```
[request]
    │
    ├─ auth middleware (jwt validation)
    ├─ rate limit check
    ├─ api version check
    │
    ▼
[cache lookup]
    │
    ├── hit → return cached response (p50: <5ms)
    │
    └── miss →
            │
            ▼
        [read replica]
            │
            ├── filtered list → composite index scan → cache result → return
            ├── nlp search → parse → same as filtered list
            ├── single profile → pk lookup (already fast)
            └── csv export → streaming cursor query → return (no cache)
```

the key insight: our nlp search (`"young males from nigeria"`) is just keyword mapping to structured filters. once parsed, it follows the exact same path as a regular filtered list query. this means the same indexes and cache serve both endpoints.

### write path

```
[create profile request]
    │
    ├─ admin role check
    │
    ▼
[check if name exists] → primary db
    │
    ├── exists → return existing profile
    │
    └── new →
            │
            ▼
        [call genderize, agify, nationalize] → parallel
            │
            ▼
        [insert into profiles] → primary db
            │
            ▼
        [invalidate cache for affected filter keys]
            │
            ▼
        return created profile
```

writes are infrequent (admin-only, one at a time, calling external apis). they're not the bottleneck. the bottleneck is reads.

### batch ingestion path (future)

for loading millions of profiles at once instead of one-by-one via the api:

```
[seed script / batch job]
    │
    ▼
[csv → copy to stdin] → primary db
    │
    ▼
[run analyze manually] → primary db
    │
    ▼
[invalidate entire cache]
    │
    ▼
[warm cache with top 50 queries]
```

postgresql's `copy` command loads millions of rows in seconds. much faster than individual inserts.

---

## 4. design decisions

### decision 1: composite + covering indexes on the profiles table

**problem:** our profiles table has one index — `lower(name)`. our actual query patterns filter on `gender`, `country_id`, `age_group`, `age`, and sort by `created_at`, `age`, or `gender_probability`. at 20m rows, every list query does a sequential scan.

**decision:** add composite indexes matching our real query shapes:

```sql
-- our most common query: filter by country + gender + age range, sorted by created_at
create index idx_profiles_country_gender_age_created
  on profiles (country_id, gender, age_group, age, created_at desc);

-- queries filtering by country + age (without gender)
create index idx_profiles_country_age_created
  on profiles (country_id, age_group, age, created_at desc);

-- sorted by age instead of created_at
create index idx_profiles_country_gender_age
  on profiles (country_id, gender, age_group, age);

-- single-column indexes for simple filters
create index idx_profiles_gender on profiles (gender);
create index idx_profiles_age_group on profiles (age_group);
create index idx_profiles_created_at on profiles (created_at desc);
```

**trade-off:** each index adds ~50-100mb at 10m rows and slows writes slightly. but our writes are rare and batch-oriented — the trade-off is heavily in favor of read performance.

**why not covering indexes with `include`:** our `select *` fetches all columns anyway (the list dto includes name, gender, age, country etc.). a covering index would need to include almost every column, making it nearly as large as the table itself. not worth it until we switch to sparse field selection.

**maps to requirement:** p50 < 500ms, p95 < 2s — without indexes, filtered queries at 20m rows will take seconds even for simple filters.

### decision 2: application-level query cache (moka → redis)

**problem:** ~40% of queries are repeated. without caching, every single request hits the database.

**decision:** add moka (rust in-process cache) first. cache key is the full query shape (endpoint + all filter params + sort + page + limit). ttl of 2 minutes.

```rust
let cache: Cache<String, CachedQueryResult> = Cache::builder()
    .max_capacity(10_000)
    .time_to_idle(Duration::from_secs(120))
    .build();

// cache key: "list:country=NG:gender=male:page=1:limit=10:sort=created_at:order=desc"
```

**why moka over redis:** single api instance = no need for shared cache. moka has zero operational overhead (no separate process, no network). if we scale to multiple instances, we add redis as a shared cache layer — but not before.

**cache invalidation:** after any write (create, delete, batch load), invalidate all cache entries. for a read-heavy system with rare writes, full invalidation is simpler and more correct than trying to track which keys are affected.

```rust
// after profile create or delete:
cache.invalidate_all();
```

**maps to requirement:** p50 < 500ms — cache hits return in <5ms instead of hitting the database.

### decision 3: read replica for read/write separation

**problem:** at 1000 qpm with complex filtered queries, a single postgres instance becomes cpu-bound on query planning and execution.

**decision:** add one streaming replica. route all `get` handlers to the read pool, mutation handlers to the write pool.

```rust
pub struct AppState {
    pub db: PgPool,         // primary — for writes
    pub read_db: PgPool,    // replica — for reads
    pub jwt_secret: String,
    pub github_client_id: String,
    pub github_client_secret: String,
    pub base_url: String,
    pub cache: Cache<String, CachedQueryResult>,
}
```

streaming replication lag is typically <10ms — negligible for demographic data.

**trade-off:** slight operational complexity (monitoring replication lag, failing over if primary dies). but this is standard postgres ops, not exotic.

**why not multiple replicas yet:** one replica already doubles read capacity. we'd add a second replica only if we saturate the first, which won't happen until well past 10m rows and sustained 500+ qps.

**maps to requirement:** query throughput of 100-1000 qpm — read replica absorbs the read load.

### decision 4: fix the count query

**problem:** our `list_profiles` service runs `select count(*) from profiles where ...` on every request. at 20m rows with filters, this scan alone can take 500ms+. combined with the data query, we blow past the p95 target.

**decision three-pronged approach:**

1. **cache the count alongside the data.** since we're already caching query results, the count comes free.
2. **make the count optional.** add `?include_count=false` that skips the count query entirely. the cli and portal can opt out for subsequent pages (they already know the total from page 1).
3. **for unfiltered queries, use postgres's approximate count:**

```sql
select reltuples::bigint from pg_class where relname = 'profiles';
```

this returns instantly and is accurate to within a few percent after `analyze`.

**maps to requirement:** p50 < 500ms — the count query is often slower than the data query itself at scale.

### decision 5: streaming csv export

**problem:** our current `export_profiles_csv` does `fetch_all` — it loads every matching row into memory, then writes csv. at 20m rows, this is an oom bomb.

**decision:** use sqlx's `fetch` cursor instead of `fetch_all`:

```rust
let mut stream = sqlx::query_as::<_, Profile>(&data_sql)
    .fetch(db);  // returns a Stream, not a Vec

while let Some(row) = stream.try_next().await? {
    wtr.serialize(ProfileCsvRow::from(&row))?;
}
```

this processes rows one at a time — constant memory regardless of result size.

**trade-off:** slightly slower for small result sets (streaming overhead). but at scale, it's the difference between working and crashing.

**maps to requirement:** system must remain reliable under load — oom on export is a reliability failure.

### decision 6: aggressive autovacuum for the profiles table

**problem:** at 20m rows, stale statistics cause postgres's query planner to make catastrophically bad decisions (e.g., sequential scan instead of index scan). this happens silently and is hard to debug.

**decision:**

```sql
alter table profiles set (
    autovacuum_analyze_scale_factor = 0.02,
    autovacuum_vacuum_scale_factor = 0.05
);
```

also run `analyze` manually after batch loads.

**maps to requirement:** p95 < 2s — bad query plans are the #1 cause of latency spikes at scale.

### decision 7: observability — prometheus metrics endpoint

**problem:** our current logging tells us method/endpoint/status/time per request, but we can't slice it. we can't answer "what's the p95 for country-filtered queries?" or "what's our cache hit rate?"

**decision:** add a `/metrics` endpoint using the `metrics` + `metrics-exporter-prometheus` crates. track:

| metric | why |
|--------|-----|
| `profiles_query_duration_seconds` (histogram) | latency percentiles per query type |
| `profiles_cache_hits_total` / `profiles_cache_misses_total` | cache effectiveness |
| `profiles_db_query_duration_seconds` (histogram) | db-specific latency |
| `auth_requests_total` (counter by endpoint) | auth load |
| `db_pool_connections_active` (gauge) | pool saturation |

scrape with prometheus, dashboard with grafana.

**maps to requirement:** availability 99.9% — you can't maintain what you can't measure.

---

## 5. trade-offs and limitations

### what this design handles well
- **read-heavy query workloads** at 10-30m rows with structured filters — this is postgres's sweet spot
- **repeated query patterns** — moka cache absorbs ~40% of traffic
- **batch data loads** — copy + analyze + cache warm keeps the system responsive after ingestion
- **eventual consistency** — streaming replication lag is negligible for demographic data

### what this design doesn't handle well

| limitation | why it's acceptable | when it would matter |
|------------|-------------------|---------------------|
| **no geographic redundancy** — single region | the task specifies single-region | if the team goes multi-region or the region has an outage |
| **in-process cache doesn't survive restarts** — cache is empty after deploy | cache warms within minutes naturally | if deploy frequency is high and warm-up time is unacceptable |
| **in-memory rate limiting doesn't scale to multiple instances** | we're running one instance | if we horizontally scale the api server |
| **in-memory pkce/auth code stores don't scale** | same — single instance | if we need multiple api instances behind a load balancer |
| **offset pagination degrades on deep pages** | most users browse the first few pages | if users or scrapers try to access page 10,000+ |
| **no full-text search** — our nlp parser is rule-based, not fuzzy | the task explicitly says no ai/llm | if users need typo tolerance or semantic search |
| **no real-time analytics** — aggregations still hit postgres | the data is read-heavy but not real-time | if users need live dashboards with sub-second aggregation updates |

### what was intentionally simplified

1. **no keyset pagination** — our current offset pagination with page/limit is in the stage 3 trd spec and used by both cli and portal. switching to cursor-based pagination would break the existing contract. instead, we cap the max offset (e.g., 10,000 rows) and rely on caching for repeated page views.

2. **no database partitioning yet** — at 10-30m rows, proper composite indexes + read replica + caching is sufficient. partitioning adds schema complexity and makes cross-partition queries slower. we'd partition by `country_id` only if single-table queries degrade despite indexes (likely at 50m+ rows).

3. **no redis yet** — moka is simpler and sufficient for a single api instance. we add redis when we add a second instance.

4. **no elasticsearch** — our search is structured, not full-text. postgres with composite indexes handles it fine. we'd add es only if keyword search latency exceeds the p95 target despite indexes.

---

## bonus: future evolution

### real-time analytics

if the team needs live dashboards (e.g., "how many profiles were added in the last hour by country"), postgres isn't great for real-time aggregations over millions of rows. the evolution path:

1. **materialized views** for pre-computed aggregations — refresh every 5 minutes
2. **clickhouse** as an analytics layer if materialized views aren't fast enough — postgres remains the source of truth, clickhouse receives a replication stream for aggregation queries
3. **apache kafka + stream processor** if we need sub-second aggregation updates — but this is significant complexity and only justified if the business genuinely needs real-time

for now, cached query results + materialized views are enough.

### true natural language queries

our current nlp parser maps keywords to structured filters using pattern matching. it can't handle:
- "show me demographics for tech workers" (no "tech worker" keyword)
- "compare male vs female distributions in west africa" (comparison queries)
- "what's the average age in nigeria?" (aggregation queries)

the evolution path:
1. **intent classification + slot filling** — a lightweight model classifies the query intent (filter, aggregate, compare) and extracts parameters. still structured, but handles more phrasing variety.
2. **text-to-sql** — a small language model translates natural language to sql. risky for production (hallucinated sql), but could work with a constrained schema and validation layer.
3. **hybrid** — intent classification first, then route to either the structured engine (for filters) or text-to-sql (for complex analytics). this keeps the reliable path fast while adding flexibility incrementally.

the key constraint: any nlp evolution must produce structured queries that can use our indexes. unstructured search (elasticsearch) is a different problem entirely.
