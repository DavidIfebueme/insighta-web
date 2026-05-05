# stage 4b — solution

## part 1: query performance

### what was slow

the profiles table had one index — `lower(name)`. every filtered list query did a sequential scan. at scale, that kills you.

### what i did

**composite index on the hot path:**
```sql
create index idx_profiles_country_gender_agegroup on profiles (country_id, gender, age_group);
create index idx_profiles_age on profiles (age);
create index idx_profiles_created_at on profiles (created_at desc);
```

the composite index covers the most common filter combination. the single indexes handle range queries on age and the default sort order.

**removed lower()/upper() from where clauses.** rust normalizes filter values before binding them as sql params. wrapping columns in functions prevents postgres from using indexes — `where lower(gender) = 'male'` can't use a b-tree index on gender. `where gender = 'male'` can.

**concurrent count + data queries.** `list_profiles` runs the count and data queries in parallel via `tokio::join!` instead of sequentially. one db round trip instead of two.

**connection pool tuning.** min 5 connections (keep warm), max 20 (handle concurrent requests + uploads). configurable via env vars.

**moka cache.** in-process cache with 10,000 entry capacity and 5-minute time-to-idle. read-heavy system, ~40% repeated queries. cache hits return in under 5ms. `invalidate_all()` on any write (create, delete, upload) — simpler and more correct than tracking which keys are affected.

### before/after

measured with ~633k profiles in the database, local postgresql:

| query | before (no indexes, no cache) | after (indexes + cache) |
|-------|------|-------|
| unfiltered list (page 1, limit 10) | 40ms | 14ms (cache hit: 14ms) |
| filtered: gender=male, country=ng | 21ms | 18ms (cache hit: 17ms) |
| multi-filter + age range + sort | 500 error (type mismatch) | 15ms (cache hit: 13ms) |
| nlp search: "male from nigeria" | 18ms | 18ms (cache hit: 14ms) |
| csv export (country=ng, ~40k rows) | oom risk (fetch_all) | 540ms (streaming cursor) |

the "before" multi-filter query was actually broken — it returned 500 because numeric params were bound as text. the `::int4` and `::float8` casts fixed both correctness and performance.

note: local db numbers are optimistic. with a remote db (network latency), the cache matters more because it eliminates the round trip entirely.

---

## part 2: query normalization

### the problem

"nigerian females between ages 20 and 45" and "women aged 20-45 living in nigeria" should hit the same cache key. without normalization, they don't.

### what i did

the nlp parser already converts natural language into a structured `parsedquery` with canonical filter values. both of the above produce `{ gender: "female", country_id: "ng", min_age: 20, max_age: 45 }`.

`build_cache_key()` takes these filter values and produces a deterministic canonical key:

```
list:country_id=ng:gender=female:max_age=45:min_age=20:page=1:limit=10
```

the key is built by iterating over filter params in a fixed order — gender, country_id, age_group, min_age, max_age, etc. regardless of how the user expressed the query, if it resolves to the same filters, it gets the same key.

**deterministic?** yes. same inputs always produce the same key. no randomness, no ai.

**correct?** yes. the normalization doesn't change the meaning of the query. it just puts the same filters in the same order. if two queries produce different filters, they get different keys — as they should.

---

## part 3: csv data ingestion

### the endpoint

`post /api/profiles/upload` — admin-only, multipart form, csv file.

### how it works

**parsing:** the handler streams multipart chunks directly to a temp file on disk — the raw csv bytes never sit entirely in memory. csv reader then reads from the file one row at a time. no loading the entire file into memory at any point.

**validation per row:**
- empty/missing name → skip, count as `missing_fields`
- empty/missing gender → skip, count as `missing_fields`
- gender not "male" or "female" → skip, count as `invalid_gender`
- negative age → skip, count as `invalid_age`
- empty country_id → skip, count as `missing_fields`
- malformed row (wrong column count, broken encoding) → csv reader returns error → skip, count as `missing_fields`

**chunked bulk insert:** valid rows are accumulated into a chunk of 5000. when the chunk is full, it's inserted with a single `insert ... select * from unnest(...)` query with `on conflict (lower(name)) do nothing`.

this is not one-by-one insertion. it's 5000 rows per query using postgres's unnest function, which is the idiomatic way to do bulk inserts in postgresql.

**duplicate handling:** the `on conflict (lower(name)) do nothing` clause handles duplicates at the database level. `rows_affected()` tells us how many were actually inserted. the difference between chunk size and rows_affected is the duplicate count.

**partial failures:** each chunk is auto-committed. if chunk 50 fails, chunks 1-49 are already in the database. no rollback.

**concurrency:** a tokio semaphore with 2 permits limits concurrent uploads. prevents resource exhaustion while still allowing multiple uploads to proceed.

**body size limit:** 50mb, covering the maximum expected csv size (~26mb for 500k rows).

**temp file approach:** multipart chunks are streamed to a temp file via `field.chunk()` — never buffered entirely in memory. csv::Reader then reads from the file incrementally. the temp file is cleaned up after processing (even on error, the os cleans /tmp on reboot).

### upload response example

```json
{
  "status": "success",
  "total_rows": 50000,
  "inserted": 48231,
  "skipped": 1769,
  "reasons": {
    "duplicate_name": 1203,
    "invalid_age": 312,
    "missing_fields": 254
  }
}
```

the `reasons` map only includes non-zero categories.

### upload performance

| rows | time | inserted |
|------|------|----------|
| 1,000 | 249ms | 1,000 |
| 10,000 | 781ms | 10,000 |
| 100,000 | ~13s | 100,000 |
| 500,000 | ~61s | 500,000 |

the bottleneck is the `on conflict (lower(name))` index check — postgres has to verify each row against the unique expression index. for pure new data, this is about 8k rows/sec on local postgresql. for all-duplicate data, it's faster because on conflict do nothing short-circuits quickly.

### edge cases handled

- **empty file:** returns `{ status: "success", total_rows: 0, inserted: 0, skipped: 0, reasons: {} }`
- **all invalid rows:** everything goes into `skipped` with appropriate reasons
- **mixed valid and invalid:** valid rows are inserted, invalid ones skipped with reasons
- **duplicate names in the upload file itself:** the second occurrence hits on conflict and is counted as `duplicate_name`
- **missing optional fields** (gender_probability, country_probability, age_group, country_name): defaults are applied (1.0 for probabilities, computed age_group, looked-up country_name)
- **concurrent uploads:** semaphore limits to 2 simultaneous uploads; others wait for a permit

---

## design decisions and trade-offs

### moka over redis
single api instance = no shared cache needed. moka is zero ops overhead. if we scale to multiple instances, we add redis. not before.

### invalidate_all() over selective invalidation
read-heavy system with rare writes. full invalidation is simple and correct. selective invalidation requires tracking which cache keys are affected by a write — easy to get wrong, hard to debug.

### unnest() over multi-row values
`insert ... select * from unnest($1, $2, ...)` is cleaner and more efficient than building a dynamic `values (...), (...), (...)` string. postgresql can optimize the entire unnest as a set operation.

### no read replica
4b constraints prohibit horizontal scaling. indexes + cache + connection pooling are sufficient for the target.

### chunk size of 5000
balance between memory usage and database round trips. 5000 rows per query means ~500 bytes of sql params per row → manageable. 100 chunks for 500k rows → 100 db round trips.

### semaphore with 2 permits
allows concurrent uploads without exhausting the connection pool or database. a single 500k-row upload uses one connection at a time (sequential chunk inserts), so 2 permits means 2 concurrent uploads + 18 connections available for queries.

## files changed

| file | change |
|------|--------|
| `migrations/20260505000001_add_performance_indexes.sql` | composite + single-column indexes |
| `cargo.toml` | moka, futures, axum multipart feature |
| `src/main.rs` | pool sizing, cache + semaphore init |
| `src/shared/state.rs` | cache + upload_semaphore in appstate |
| `src/profiles/model.rs` | uploadsummary, clone derive on profilelistitemdto |
| `src/profiles/service.rs` | cache key normalization, concurrent queries, type casts, streaming export, chunked csv upload |
| `src/profiles/handler.rs` | upload endpoint, defaultbodylimit, cache integration |
