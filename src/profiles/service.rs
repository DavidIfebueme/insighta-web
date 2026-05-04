use crate::shared::country::country_id_to_name;
use crate::shared::error::AppError;
use crate::profiles::model::{
    AgifyResponse, CreateProfileRequest, GenderizeResponse, NationalizeResponse, Profile,
    ProfileDetailDto, ProfileListItemDto,
};
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

pub async fn create_profile(
    db: &PgPool,
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

    let existing = sqlx::query_as::<_, Profile>(
        "SELECT * FROM profiles WHERE LOWER(name) = $1",
    )
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
    let sample_size = genderize.count as i32;
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
        INSERT INTO profiles (id, name, gender, gender_probability, sample_size, age, age_group, country_id, country_name, country_probability, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(&name_lower)
    .bind(&gender)
    .bind(gender_probability)
    .bind(sample_size)
    .bind(age)
    .bind(&age_group)
    .bind(&country_id)
    .bind(&country_name)
    .bind(country_probability)
    .bind(created_at)
    .fetch_one(db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

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

pub async fn list_profiles(
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
    page: i64,
    limit: i64,
) -> Result<(i64, Vec<ProfileListItemDto>), AppError> {
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

    let mut conditions = Vec::new();
    let mut param_idx = 1u32;

    let gender_val = gender.map(|g| g.to_lowercase());
    let country_id_val = country_id.map(|c| c.to_uppercase());
    let age_group_val = age_group.map(|a| a.to_lowercase());

    if gender_val.is_some() {
        conditions.push(format!("LOWER(gender) = ${}", param_idx));
        param_idx += 1;
    }
    if country_id_val.is_some() {
        conditions.push(format!("UPPER(country_id) = ${}", param_idx));
        param_idx += 1;
    }
    if age_group_val.is_some() {
        conditions.push(format!("LOWER(age_group) = ${}", param_idx));
        param_idx += 1;
    }
    if min_age.is_some() {
        conditions.push(format!("age >= ${}", param_idx));
        param_idx += 1;
    }
    if max_age.is_some() {
        conditions.push(format!("age <= ${}", param_idx));
        param_idx += 1;
    }
    if min_gender_probability.is_some() {
        conditions.push(format!("gender_probability >= ${}", param_idx));
        param_idx += 1;
    }
    if min_country_probability.is_some() {
        conditions.push(format!("country_probability >= ${}", param_idx));
        param_idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let count_sql = format!("SELECT COUNT(*) as count FROM profiles {}", where_clause);
    let data_sql = format!(
        "SELECT * FROM profiles {} ORDER BY {} {} LIMIT ${} OFFSET ${}",
        where_clause, sort_col, order_dir, param_idx, param_idx + 1
    );

    let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
    let mut data_q = sqlx::query_as::<_, Profile>(&data_sql);

    if let Some(ref g) = gender_val {
        count_q = count_q.bind(g);
        data_q = data_q.bind(g);
    }
    if let Some(ref c) = country_id_val {
        count_q = count_q.bind(c);
        data_q = data_q.bind(c);
    }
    if let Some(ref a) = age_group_val {
        count_q = count_q.bind(a);
        data_q = data_q.bind(a);
    }
    if let Some(ma) = min_age {
        count_q = count_q.bind(ma);
        data_q = data_q.bind(ma);
    }
    if let Some(ma) = max_age {
        count_q = count_q.bind(ma);
        data_q = data_q.bind(ma);
    }
    if let Some(mp) = min_gender_probability {
        count_q = count_q.bind(mp);
        data_q = data_q.bind(mp);
    }
    if let Some(mp) = min_country_probability {
        count_q = count_q.bind(mp);
        data_q = data_q.bind(mp);
    }

    let total = count_q.fetch_one(db).await.map_err(|e| AppError::Internal(e.into()))?;

    data_q = data_q.bind(limit).bind((page - 1) * limit);
    let profiles = data_q.fetch_all(db).await.map_err(|e| AppError::Internal(e.into()))?;

    Ok((total, profiles.into_iter().map(ProfileListItemDto::from).collect()))
}

pub async fn delete_profile(db: &PgPool, id: Uuid) -> Result<(), AppError> {
    let result = sqlx::query("DELETE FROM profiles WHERE id = $1")
        .bind(id)
        .execute(db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Profile not found".to_string()));
    }

    Ok(())
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

async fn fetch_agify(
    client: &reqwest::Client,
    name: &str,
) -> Result<AgifyResponse, AppError> {
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
