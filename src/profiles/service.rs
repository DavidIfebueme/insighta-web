use crate::profiles::model::{
    AgifyResponse, CreateProfileRequest, GenderizeResponse, NationalizeResponse, Profile,
    ProfileDetailDto, ProfileListItemDto, UploadSummary,
};
use crate::shared::country::country_id_to_name;
use crate::shared::error::AppError;
use futures::TryStreamExt;
use moka::sync::Cache;
use sqlx::PgPool;
use uuid::Uuid;

fn classify_age_group(age: i32) -> &'static str {
    match age {
        0..=12 => "child",
        13..=19 => "teenager",
        20..=59 => "adult",
        _ => "senior",
    }
}

#[derive(Debug, Clone)]
pub struct CachedQueryResult {
    pub total: i64,
    pub data: Vec<ProfileListItemDto>,
}

pub fn build_cache_key(
    prefix: &str,
    gender: Option<&str>,
    country_id: Option<&str>,
    age_group: Option<&str>,
    min_age: Option<i32>,
    max_age: Option<i32>,
    min_gender_probability: Option<f64>,
    min_country_probability: Option<f64>,
    sort_by: Option<&str>,
    order: Option<&str>,
    page: i64,
    limit: i64,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(prefix.to_string());
    if let Some(g) = gender {
        parts.push(format!("gender={}", g));
    }
    if let Some(c) = country_id {
        parts.push(format!("country_id={}", c));
    }
    if let Some(a) = age_group {
        parts.push(format!("age_group={}", a));
    }
    if let Some(ma) = min_age {
        parts.push(format!("min_age={}", ma));
    }
    if let Some(ma) = max_age {
        parts.push(format!("max_age={}", ma));
    }
    if let Some(mp) = min_gender_probability {
        parts.push(format!("min_gender_probability={}", mp));
    }
    if let Some(mp) = min_country_probability {
        parts.push(format!("min_country_probability={}", mp));
    }
    if let Some(s) = sort_by {
        parts.push(format!("sort_by={}", s));
    }
    if let Some(o) = order {
        parts.push(format!("order={}", o));
    }
    parts.push(format!("page={}", page));
    parts.push(format!("limit={}", limit));
    parts.join(":")
}

fn build_where_clause(
    gender: Option<&str>,
    country_id: Option<&str>,
    age_group: Option<&str>,
    min_age: Option<i32>,
    max_age: Option<i32>,
    min_gender_probability: Option<f64>,
    min_country_probability: Option<f64>,
) -> (String, Vec<String>, Vec<String>) {
    let mut conditions = Vec::new();
    let mut count_params: Vec<String> = Vec::new();
    let mut data_params: Vec<String> = Vec::new();
    let mut param_idx = 1u32;

    if let Some(g) = gender {
        conditions.push(format!("gender = ${}", param_idx));
        count_params.push(g.to_string());
        data_params.push(g.to_string());
        param_idx += 1;
    }
    if let Some(c) = country_id {
        conditions.push(format!("country_id = ${}", param_idx));
        count_params.push(c.to_string());
        data_params.push(c.to_string());
        param_idx += 1;
    }
    if let Some(a) = age_group {
        conditions.push(format!("age_group = ${}", param_idx));
        count_params.push(a.to_string());
        data_params.push(a.to_string());
        param_idx += 1;
    }
    if let Some(ma) = min_age {
        conditions.push(format!("age >= ${}::int4", param_idx));
        count_params.push(ma.to_string());
        data_params.push(ma.to_string());
        param_idx += 1;
    }
    if let Some(ma) = max_age {
        conditions.push(format!("age <= ${}::int4", param_idx));
        count_params.push(ma.to_string());
        data_params.push(ma.to_string());
        param_idx += 1;
    }
    if let Some(mp) = min_gender_probability {
        conditions.push(format!("gender_probability >= ${}::float8", param_idx));
        count_params.push(mp.to_string());
        data_params.push(mp.to_string());
        param_idx += 1;
    }
    if let Some(mp) = min_country_probability {
        conditions.push(format!("country_probability >= ${}::float8", param_idx));
        count_params.push(mp.to_string());
        data_params.push(mp.to_string());
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    (where_clause, count_params, data_params)
}

async fn execute_count(db: &PgPool, sql: &str, params: &[String]) -> Result<i64, AppError> {
    let mut q = sqlx::query_scalar::<_, i64>(sql);
    for p in params {
        q = q.bind(p);
    }
    q.fetch_one(db)
        .await
        .map_err(|e| AppError::Internal(e.into()))
}

async fn execute_data(
    db: &PgPool,
    sql: &str,
    params: &[String],
    limit: i64,
    offset: i64,
) -> Result<Vec<Profile>, AppError> {
    let mut q = sqlx::query_as::<_, Profile>(sql);
    for p in params {
        q = q.bind(p);
    }
    q = q.bind(limit).bind(offset);
    q.fetch_all(db)
        .await
        .map_err(|e| AppError::Internal(e.into()))
}

pub async fn create_profile(
    db: &PgPool,
    cache: &Cache<String, CachedQueryResult>,
    req: CreateProfileRequest,
) -> Result<(ProfileDetailDto, bool), AppError> {
    let name = req
        .name
        .ok_or_else(|| AppError::BadRequest("Name is required".to_string()))?;
    let name_trimmed = name.trim().to_string();
    let name_lower = name_trimmed.to_lowercase();

    if name_lower.is_empty() {
        return Err(AppError::BadRequest("Name is required".to_string()));
    }

    let existing = sqlx::query_as::<_, Profile>("SELECT * FROM profiles WHERE LOWER(name) = $1")
        .bind(&name_lower)
        .fetch_optional(db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    if let Some(profile) = existing {
        return Ok((ProfileDetailDto::from(profile), true));
    }

    let client = reqwest::Client::new();

    let (genderize, agify, nationalize) = tokio::try_join!(
        fetch_genderize(&client, &name_lower),
        fetch_agify(&client, &name_lower),
        fetch_nationalize(&client, &name_lower),
    )?;

    let gender = genderize.gender.unwrap();
    let gender_probability = genderize.probability;
    let age = agify.age.unwrap() as i32;
    let age_group = classify_age_group(age).to_string();

    let top_country = nationalize
        .country
        .iter()
        .max_by(|a, b| {
            a.probability
                .partial_cmp(&b.probability)
                .unwrap_or(std::cmp::Ordering::Less)
        })
        .unwrap();

    let country_id = top_country.country_id.clone();
    let country_name = country_id_to_name(&country_id);
    let country_probability = top_country.probability;

    let id = Uuid::now_v7();
    let created_at = chrono::Utc::now();

    let profile = sqlx::query_as::<_, Profile>(
        r#"
        INSERT INTO profiles (id, name, gender, gender_probability, age, age_group, country_id, country_name, country_probability, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(&name_lower)
    .bind(&gender)
    .bind(gender_probability)
    .bind(age)
    .bind(&age_group)
    .bind(&country_id)
    .bind(&country_name)
    .bind(country_probability)
    .bind(created_at)
    .fetch_one(db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    cache.invalidate_all();

    Ok((ProfileDetailDto::from(profile), false))
}

pub async fn get_profile(db: &PgPool, id: Uuid) -> Result<ProfileDetailDto, AppError> {
    let profile = sqlx::query_as::<_, Profile>("SELECT * FROM profiles WHERE id = $1")
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(|| AppError::NotFound("Profile not found".to_string()))?;

    Ok(ProfileDetailDto::from(profile))
}

#[allow(clippy::too_many_arguments)]
pub async fn list_profiles(
    db: &PgPool,
    cache: &Cache<String, CachedQueryResult>,
    gender: Option<String>,
    country_id: Option<String>,
    age_group: Option<String>,
    min_age: Option<i32>,
    max_age: Option<i32>,
    min_gender_probability: Option<f64>,
    min_country_probability: Option<f64>,
    sort_by: Option<String>,
    order: Option<String>,
    page: i64,
    limit: i64,
) -> Result<(i64, Vec<ProfileListItemDto>), AppError> {
    let gender_val = gender.map(|g| g.to_lowercase());
    let country_id_val = country_id.map(|c| c.to_uppercase());
    let age_group_val = age_group.map(|a| a.to_lowercase());

    let cache_key = build_cache_key(
        "list",
        gender_val.as_deref(),
        country_id_val.as_deref(),
        age_group_val.as_deref(),
        min_age,
        max_age,
        min_gender_probability,
        min_country_probability,
        sort_by.as_deref(),
        order.as_deref(),
        page,
        limit,
    );

    if let Some(cached) = cache.get(&cache_key) {
        return Ok((cached.total, cached.data));
    }

    let sort_col = match sort_by.as_deref() {
        Some("age") => "age",
        Some("gender_probability") => "gender_probability",
        Some("created_at") => "created_at",
        None => "created_at",
        _ => return Err(AppError::BadRequest("Invalid query parameters".to_string())),
    };

    let order_dir = match order.as_deref() {
        Some("asc") => "ASC",
        Some("desc") => "DESC",
        None => "DESC",
        _ => return Err(AppError::BadRequest("Invalid query parameters".to_string())),
    };

    let (where_clause, count_params, data_params) = build_where_clause(
        gender_val.as_deref(),
        country_id_val.as_deref(),
        age_group_val.as_deref(),
        min_age,
        max_age,
        min_gender_probability,
        min_country_probability,
    );

    let count_sql = format!("SELECT COUNT(*) as count FROM profiles {}", where_clause);
    let param_idx = count_params.len() as u32 + 1;
    let data_sql = format!(
        "SELECT * FROM profiles {} ORDER BY {} {} LIMIT ${} OFFSET ${}",
        where_clause,
        sort_col,
        order_dir,
        param_idx,
        param_idx + 1
    );

    let offset = (page - 1) * limit;

    let (total, profiles) = tokio::join!(
        execute_count(db, &count_sql, &count_params),
        execute_data(db, &data_sql, &data_params, limit, offset),
    );

    let total = total?;
    let profiles = profiles?;

    let data: Vec<ProfileListItemDto> = profiles.into_iter().map(ProfileListItemDto::from).collect();

    cache.insert(
        cache_key,
        CachedQueryResult {
            total,
            data: data.clone(),
        },
    );

    Ok((total, data))
}

pub async fn delete_profile(
    db: &PgPool,
    cache: &Cache<String, CachedQueryResult>,
    id: Uuid,
) -> Result<(), AppError> {
    let result = sqlx::query("DELETE FROM profiles WHERE id = $1")
        .bind(id)
        .execute(db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Profile not found".to_string()));
    }

    cache.invalidate_all();

    Ok(())
}

pub async fn export_profiles_csv(
    db: &PgPool,
    gender: Option<String>,
    country_id: Option<String>,
    age_group: Option<String>,
    min_age: Option<i32>,
    max_age: Option<i32>,
    min_gender_probability: Option<f64>,
    min_country_probability: Option<f64>,
    sort_by: Option<String>,
    order: Option<String>,
) -> Result<String, AppError> {
    let gender_val = gender.map(|g| g.to_lowercase());
    let country_id_val = country_id.map(|c| c.to_uppercase());
    let age_group_val = age_group.map(|a| a.to_lowercase());

    let sort_col = match sort_by.as_deref() {
        Some("age") => "age",
        Some("gender_probability") => "gender_probability",
        Some("created_at") => "created_at",
        None => "created_at",
        _ => return Err(AppError::BadRequest("Invalid query parameters".to_string())),
    };

    let order_dir = match order.as_deref() {
        Some("asc") => "ASC",
        Some("desc") => "DESC",
        None => "DESC",
        _ => return Err(AppError::BadRequest("Invalid query parameters".to_string())),
    };

    let (where_clause, _, data_params) = build_where_clause(
        gender_val.as_deref(),
        country_id_val.as_deref(),
        age_group_val.as_deref(),
        min_age,
        max_age,
        min_gender_probability,
        min_country_probability,
    );

    let data_sql = format!(
        "SELECT * FROM profiles {} ORDER BY {} {}",
        where_clause, sort_col, order_dir
    );

    let mut q = sqlx::query_as::<_, Profile>(&data_sql);
    for p in &data_params {
        q = q.bind(p);
    }

    let mut stream = q.fetch(db);

    let mut wtr = csv::Writer::from_writer(Vec::new());
    while let Some(row) = stream
        .try_next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        wtr.serialize(ProfileCsvRow::from(&row))
            .map_err(|e| AppError::Internal(e.into()))?;
    }

    let bytes = wtr.into_inner().map_err(|e| AppError::Internal(e.into()))?;
    String::from_utf8(bytes).map_err(|e| AppError::Internal(e.into()))
}

pub async fn upload_profiles(
    db: &PgPool,
    cache: &Cache<String, CachedQueryResult>,
    csv_data: &[u8],
) -> Result<UploadSummary, AppError> {
    let mut rdr = csv::Reader::from_reader(csv_data);
    let mut total_rows: i64 = 0;
    let mut skipped: i64 = 0;
    let mut duplicate_name: i64 = 0;
    let mut invalid_age: i64 = 0;
    let mut missing_fields: i64 = 0;
    let mut invalid_gender: i64 = 0;
    let mut inserted: i64 = 0;

    let chunk_size = 5000;
    let mut chunk: Vec<UploadRow> = Vec::with_capacity(chunk_size);

    for result in rdr.deserialize() {
        let row: UploadRow = match result {
            Ok(r) => r,
            Err(_) => {
                total_rows += 1;
                missing_fields += 1;
                skipped += 1;
                continue;
            }
        };

        total_rows += 1;

        if row.name.trim().is_empty() {
            missing_fields += 1;
            skipped += 1;
            continue;
        }

        if row.gender.is_empty() {
            missing_fields += 1;
            skipped += 1;
            continue;
        }

        let gender_lower = row.gender.to_lowercase();
        if gender_lower != "male" && gender_lower != "female" {
            invalid_gender += 1;
            skipped += 1;
            continue;
        }

        if row.age < 0 {
            invalid_age += 1;
            skipped += 1;
            continue;
        }

        let country_upper = row.country_id.to_uppercase();
        if country_upper.is_empty() {
            missing_fields += 1;
            skipped += 1;
            continue;
        }

        let age_group = if row.age_group.is_empty() {
            classify_age_group(row.age).to_string()
        } else {
            row.age_group.to_lowercase()
        };

        let country_name = if row.country_name.is_empty() {
            country_id_to_name(&country_upper)
        } else {
            row.country_name
        };

        let gp = if row.gender_probability <= 0.0 {
            1.0
        } else {
            row.gender_probability
        };

        let cp = if row.country_probability <= 0.0 {
            1.0
        } else {
            row.country_probability
        };

        chunk.push(UploadRow {
            name: row.name.trim().to_lowercase(),
            gender: gender_lower,
            gender_probability: gp,
            age: row.age,
            age_group,
            country_id: country_upper,
            country_name,
            country_probability: cp,
        });

        if chunk.len() >= chunk_size {
            let ins = insert_chunk(db, &chunk).await?;
            let dups = chunk.len() as i64 - ins;
            inserted += ins;
            duplicate_name += dups;
            skipped += dups;
            chunk.clear();
        }
    }

    if !chunk.is_empty() {
        let ins = insert_chunk(db, &chunk).await?;
        let dups = chunk.len() as i64 - ins;
        inserted += ins;
        duplicate_name += dups;
        skipped += dups;
    }

    cache.invalidate_all();

    let mut reasons = serde_json::Map::new();
    if duplicate_name > 0 {
        reasons.insert(
            "duplicate_name".to_string(),
            serde_json::Value::Number(duplicate_name.into()),
        );
    }
    if invalid_age > 0 {
        reasons.insert(
            "invalid_age".to_string(),
            serde_json::Value::Number(invalid_age.into()),
        );
    }
    if missing_fields > 0 {
        reasons.insert(
            "missing_fields".to_string(),
            serde_json::Value::Number(missing_fields.into()),
        );
    }
    if invalid_gender > 0 {
        reasons.insert(
            "invalid_gender".to_string(),
            serde_json::Value::Number(invalid_gender.into()),
        );
    }

    Ok(UploadSummary {
        status: "success".to_string(),
        total_rows,
        inserted,
        skipped,
        reasons,
    })
}

async fn insert_chunk(db: &PgPool, rows: &[UploadRow]) -> Result<i64, AppError> {
    if rows.is_empty() {
        return Ok(0);
    }

    let now = chrono::Utc::now();

    let ids: Vec<Uuid> = rows.iter().map(|_| Uuid::now_v7()).collect();
    let names: Vec<String> = rows.iter().map(|r| r.name.clone()).collect();
    let genders: Vec<String> = rows.iter().map(|r| r.gender.clone()).collect();
    let gender_probs: Vec<f64> = rows.iter().map(|r| r.gender_probability).collect();
    let ages: Vec<i32> = rows.iter().map(|r| r.age).collect();
    let age_groups: Vec<String> = rows.iter().map(|r| r.age_group.clone()).collect();
    let country_ids: Vec<String> = rows.iter().map(|r| r.country_id.clone()).collect();
    let country_names: Vec<String> = rows.iter().map(|r| r.country_name.clone()).collect();
    let country_probs: Vec<f64> = rows.iter().map(|r| r.country_probability).collect();
    let created_ats: Vec<chrono::DateTime<chrono::Utc>> = rows.iter().map(|_| now).collect();

    let result = sqlx::query(
        r#"
        INSERT INTO profiles (id, name, gender, gender_probability, age, age_group, country_id, country_name, country_probability, created_at)
        SELECT * FROM UNNEST(
            $1::uuid[], $2::varchar[], $3::varchar[], $4::float8[], $5::int4[],
            $6::varchar[], $7::varchar[], $8::varchar[], $9::float8[], $10::timestamptz[]
        )
        ON CONFLICT (LOWER(name)) DO NOTHING
        "#,
    )
    .bind(&ids)
    .bind(&names)
    .bind(&genders)
    .bind(&gender_probs)
    .bind(&ages)
    .bind(&age_groups)
    .bind(&country_ids)
    .bind(&country_names)
    .bind(&country_probs)
    .bind(&created_ats)
    .execute(db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(result.rows_affected() as i64)
}

#[derive(Debug, serde::Deserialize)]
struct UploadRow {
    name: String,
    gender: String,
    gender_probability: f64,
    age: i32,
    age_group: String,
    country_id: String,
    country_name: String,
    country_probability: f64,
}

#[derive(serde::Serialize)]
struct ProfileCsvRow {
    id: String,
    name: String,
    gender: String,
    gender_probability: f64,
    age: i32,
    age_group: String,
    country_id: String,
    country_name: String,
    country_probability: f64,
    created_at: String,
}

impl From<&Profile> for ProfileCsvRow {
    fn from(p: &Profile) -> Self {
        Self {
            id: p.id.to_string(),
            name: p.name.clone(),
            gender: p.gender.clone(),
            gender_probability: p.gender_probability,
            age: p.age,
            age_group: p.age_group.clone(),
            country_id: p.country_id.clone(),
            country_name: p.country_name.clone(),
            country_probability: p.country_probability,
            created_at: p.created_at.to_rfc3339(),
        }
    }
}

async fn fetch_genderize(
    client: &reqwest::Client,
    name: &str,
) -> Result<GenderizeResponse, AppError> {
    let resp = client
        .get("https://api.genderize.io")
        .query(&[("name", name)])
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Genderize request failed: {}", e);
            AppError::BadGateway("Genderize returned an invalid response".to_string())
        })?;

    if !resp.status().is_success() {
        return Err(AppError::BadGateway(
            "Genderize returned an invalid response".to_string(),
        ));
    }

    let data: GenderizeResponse = resp.json().await.map_err(|e| {
        tracing::error!("Genderize parse error: {}", e);
        AppError::BadGateway("Genderize returned an invalid response".to_string())
    })?;

    if data.gender.is_none() || data.count == 0 {
        return Err(AppError::BadGateway(
            "Genderize returned an invalid response".to_string(),
        ));
    }

    Ok(data)
}

async fn fetch_agify(client: &reqwest::Client, name: &str) -> Result<AgifyResponse, AppError> {
    let resp = client
        .get("https://api.agify.io")
        .query(&[("name", name)])
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Agify request failed: {}", e);
            AppError::BadGateway("Agify returned an invalid response".to_string())
        })?;

    if !resp.status().is_success() {
        return Err(AppError::BadGateway(
            "Agify returned an invalid response".to_string(),
        ));
    }

    let data: AgifyResponse = resp.json().await.map_err(|e| {
        tracing::error!("Agify parse error: {}", e);
        AppError::BadGateway("Agify returned an invalid response".to_string())
    })?;

    if data.age.is_none() {
        return Err(AppError::BadGateway(
            "Agify returned an invalid response".to_string(),
        ));
    }

    Ok(data)
}

async fn fetch_nationalize(
    client: &reqwest::Client,
    name: &str,
) -> Result<NationalizeResponse, AppError> {
    let resp = client
        .get("https://api.nationalize.io")
        .query(&[("name", name)])
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Nationalize request failed: {}", e);
            AppError::BadGateway("Nationalize returned an invalid response".to_string())
        })?;

    if !resp.status().is_success() {
        return Err(AppError::BadGateway(
            "Nationalize returned an invalid response".to_string(),
        ));
    }

    let data: NationalizeResponse = resp.json().await.map_err(|e| {
        tracing::error!("Nationalize parse error: {}", e);
        AppError::BadGateway("Nationalize returned an invalid response".to_string())
    })?;

    if data.country.is_empty() {
        return Err(AppError::BadGateway(
            "Nationalize returned an invalid response".to_string(),
        ));
    }

    Ok(data)
}
