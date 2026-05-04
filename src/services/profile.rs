use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::{
    CreateProfileRequest, ListProfilesQuery, Profile, ProfileDetailDto, ProfileListItemDto,
};
use crate::services::external::ExternalApiService;

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
    let name_lower = name.trim().to_lowercase();

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

    let api = ExternalApiService::new();

    let (genderize, agify, nationalize) = tokio::try_join!(
        api.fetch_genderize(&name_lower),
        api.fetch_agify(&name_lower),
        api.fetch_nationalize(&name_lower),
    )?;

    let gender = genderize.gender.unwrap();
    let gender_probability = genderize.probability;
    let sample_size = genderize.count as i32;
    let age = agify.age.unwrap() as i32;
    let age_group = classify_age_group(age).to_string();

    let top_country = nationalize
        .country
        .iter()
        .max_by(|a, b| a.probability.partial_cmp(&b.probability).unwrap_or(std::cmp::Ordering::Less))
        .unwrap();

    let country_id = top_country.country_id.clone();
    let country_probability = top_country.probability;

    let id = Uuid::now_v7();
    let created_at = Utc::now();

    let profile = sqlx::query_as::<_, Profile>(
        r#"
        INSERT INTO profiles (id, name, gender, gender_probability, sample_size, age, age_group, country_id, country_probability, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
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
    query: ListProfilesQuery,
) -> Result<Vec<ProfileListItemDto>, AppError> {
    let mut conditions = Vec::new();
    let mut param_idx = 1u32;

    let gender_val: Option<String> = query.gender.map(|g| g.to_lowercase());
    let country_id_val: Option<String> = query.country_id.map(|c| c.to_uppercase());
    let age_group_val: Option<String> = query.age_group.map(|a| a.to_lowercase());

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

    let sql = if conditions.is_empty() {
        "SELECT * FROM profiles ORDER BY created_at DESC".to_string()
    } else {
        format!(
            "SELECT * FROM profiles WHERE {} ORDER BY created_at DESC",
            conditions.join(" AND ")
        )
    };

    let mut q = sqlx::query_as::<_, Profile>(&sql);

    if let Some(ref g) = gender_val {
        q = q.bind(g);
    }
    if let Some(ref c) = country_id_val {
        q = q.bind(c);
    }
    if let Some(ref a) = age_group_val {
        q = q.bind(a);
    }

    let profiles = q.fetch_all(db).await.map_err(|e| AppError::Internal(e.into()))?;

    Ok(profiles.into_iter().map(ProfileListItemDto::from).collect())
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
