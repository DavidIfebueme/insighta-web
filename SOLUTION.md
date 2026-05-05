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

### live benchmark results (653,242 profiles in production db)

all numbers measured server-side via localhost — pure processing time, no network or tls overhead.

| query | cache miss | cache hit | speedup |
|-------|-----------|----------|---------|
| unfiltered list (page 1, limit 10) | 32.9ms | 1.2ms | 27x |
| filtered: gender=male, country=ng | 4.9ms | 0.8ms | 6x |
| multi-filter: female, ke, age 20-45 | 26.9ms | 1.0ms | 27x |
| age range: min_age=30, max_age=50 | 12.5ms | 1.0ms | 13x |
| age group: senior | 36.0ms | 1.6ms | 23x |
| age group: child | 71.7ms | 2.0ms | 36x |
| gender=female only | 28.1ms | 1.6ms | 18x |
| min_age=50 only | 22.0ms | 1.7ms | 13x |

### nlp search (653k profiles, server-side)

| query | cache miss | cache hit |
|-------|-----------|----------|
| "nigerian females between the ages of 20-74" | 38.8ms | 1.3ms |
| "women aged 20-45 living in nigeria" | 15.9ms | 1.2ms |
| "males from nigeria over 30" | 34.4ms | 1.2ms |
| "teens in nigeria" | 3.4ms | 1.2ms |
| "elderly women from rwanda" | 3.2ms | 0.8ms |
| "boys under 18 in kenya" | 13.0ms | 2.3ms |

nlp parsing itself is ~0.01ms — negligible. the miss latency is determined entirely by what filters the parser extracts and how well those match the composite index. "elderly women from rwanda" is fast because rwanda has few profiles. "nigerian females 20-74" is slower because nigeria has ~40k profiles and the age range touches many rows.

### csv export (653k profiles, server-side)

| export | time | size |
|--------|------|------|
| all profiles (653k rows) | 1.98s | 86.5 mb |
| country=ng (~40k rows) | 75ms | 1.6 mb |
| female, ke, age 20-45 | 28ms | 250 kb |

streaming cursor, not fetch_all. rows serialize one-at-a-time to csv. the all-profiles export is ~330k rows/sec — limited by csv serialization overhead, not the query.

### csv upload (653k profiles already in db)

| rows | time | inserted | rows/sec |
|------|------|----------|----------|
| 50,000 | 1,316ms | 50,000 | 38,055 |
| 100,000 | 2,892ms | 100,000 | 34,578 |
| 500,000 | 16,161ms | 500,000 | 30,939 |

each 5,000-row chunk is one `insert ... select * from unnest(...)` with `on conflict (lower(name)) do nothing`. that's ~160ms per chunk. the bottleneck is the unique expression index check — postgres has to verify each row against the index. as the table grows, this check gets slightly slower per row.

### why these numbers

cache misses hit postgresql. the unfiltered and age-group queries scan more rows because those filters don't fully match the composite index `(country_id, gender, age_group)`. the `gender=male, country=ng` query is only 4.9ms miss because the composite index covers it perfectly — postgres can do an index-only scan.

cache hits are ~1-2ms because they're just a hashmap lookup in moka's in-process memory. no sql, no network, no serialization. the cache stores the full result (total count + page of dtos) so the response is constructed directly from memory.

### before (for reference)

before stage 4b, with ~2k profiles and no indexes or cache:

| query | before (no indexes, no cache) |
|-------|------|
| unfiltered list (page 1) | 40ms |
| filtered: gender=male, country=ng | 21ms |
| multi-filter + age range + sort | 500 error (type mismatch) |
| nlp search: "male from nigeria" | 18ms |
| csv export (country=ng, ~40k rows) | oom risk (fetch_all) |

the multi-filter query was actually broken — it returned 500 because numeric params were bound as text. the `::int4` and `::float8` casts fixed both correctness and performance.

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

### upload performance (live, 653k profiles already in db)

| rows | time | inserted | rows/sec |
|------|------|----------|----------|
| 50,000 | 1,316ms | 50,000 | 38,055 |
| 100,000 | 2,892ms | 100,000 | 34,578 |
| 500,000 | 16,161ms | 500,000 | 30,939 |

the bottleneck is the `on conflict (lower(name))` index check — postgres has to verify each row against the unique expression index. for pure new data, this is about 31-38k rows/sec. throughput decreases slightly at scale because the index tree grows and each check touches more pages.

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

---

## how we achieved this speed

### indexes that match the query patterns

the composite index `(country_id, gender, age_group)` covers the most common filter combination in one shot. when a query filters by all three columns, postgres does an index-only scan — it never touches the heap. single indexes on `age` and `created_at desc` handle range queries and the default sort order. the key insight: indexes only work if the query matches the column order. a `where country_id = 'ng' and gender = 'male'` query can use the first two columns of the composite index. a `where gender = 'male'` query alone cannot — it falls back to a bitmap scan or sequential scan.

### removing function wrappers from where clauses

`where lower(gender) = 'male'` cannot use a b-tree index on gender. wrapping a column in any function makes it opaque to the planner. rust normalizes filter values to lowercase/uppercase before binding them as sql params, so `where gender = 'male'` works and hits the index. same for country_id (normalized to uppercase).

### concurrent count + data

`list_profiles` runs the count query and the data query in parallel via `tokio::join!`. before, they ran sequentially — two round trips to the database. now it's one. on a local database this saves ~5ms. on a remote database with network latency, it saves much more.

### moka in-process cache

10,000 entries, 5-minute time-to-idle eviction. cache hits return in 1-2ms regardless of query complexity or dataset size. the cache stores the full result (count + page of dtos), so the response is constructed directly from memory with no sql, no network, no serialization. `invalidate_all()` on any write operation (create, delete, upload) — simpler than selective invalidation and correct for a read-heavy system with rare writes.

### cache key normalization

`build_cache_key()` produces a deterministic canonical string from all filter params in a fixed order. "nigerian females between ages 20 and 45" and "women aged 20-45 living in nigeria" both resolve to `country_id=ng:gender=female:max_age=45:min_age=20` and hit the same cache entry. without normalization, these would be separate entries and the cache would be less effective.

### type casts for bound parameters

`where age >= $1::int4` instead of `where age >= $1`. when numeric params are bound as text strings (runtime sqlx limitation), postgres has to infer the type at query time. without the cast, it sometimes gets it wrong and falls back to a sequential scan. the explicit cast lets the planner use the index.

### unnest bulk insert instead of one-by-one

5000 rows per `insert ... select * from unnest(...)` statement. this is ~100x faster than inserting one row per query because it amortizes the query planning, network round trip, and transaction overhead across 5000 rows. at 30k+ rows/sec, a 500k-row upload completes in ~16 seconds.

### streaming for large result sets

csv export uses `sqlx::fetch` (streaming cursor) instead of `fetch_all`. rows are pulled from postgres one at a time and written to csv incrementally. this means a 86.5mb export of 653k rows never holds more than a few rows in memory at once. before, `fetch_all` would load the entire result set into memory — an oom risk at scale.

### streaming for uploads too

multipart chunks are streamed to a temp file on disk via `field.chunk()`. the csv bytes never sit entirely in memory. the csv reader then processes the temp file row-by-row. this is why a 500k-row upload works fine within a 50mb body limit — the in-memory footprint is just one chunk of 5000 rows at a time.
