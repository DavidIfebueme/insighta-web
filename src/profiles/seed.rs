use crate::profiles::model::SeedData;
use crate::shared::error::AppError;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn seed_profiles(db: &PgPool) -> Result<(), AppError> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM profiles")
        .fetch_one(db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    if count.0 > 0 {
        tracing::info!("Profiles already exist ({}), skipping seed", count.0);
        return Ok(());
    }

    let data = include_str!("../../seed_profiles.json");
    let seed: SeedData =
        serde_json::from_str(data).map_err(|e| AppError::Internal(e.into()))?;

    tracing::info!("Seeding {} profiles...", seed.profiles.len());

    for profile in &seed.profiles {
        let id = Uuid::now_v7();
        sqlx::query(
            r#"
            INSERT INTO profiles (id, name, gender, gender_probability, sample_size, age, age_group, country_id, country_name, country_probability, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())
            ON CONFLICT (LOWER(name)) DO NOTHING
            "#,
        )
        .bind(id)
        .bind(&profile.name)
        .bind(&profile.gender)
        .bind(profile.gender_probability)
        .bind(0i32)
        .bind(profile.age)
        .bind(&profile.age_group)
        .bind(&profile.country_id)
        .bind(&profile.country_name)
        .bind(profile.country_probability)
        .execute(db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    }

    let final_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM profiles")
        .fetch_one(db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    tracing::info!("Seed complete. {} profiles in database.", final_count.0);

    Ok(())
}
