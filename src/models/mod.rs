use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    pub gender: String,
    pub gender_probability: f64,
    pub sample_size: i32,
    pub age: i32,
    pub age_group: String,
    pub country_id: String,
    pub country_probability: f64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateProfileRequest {
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProfileDetailDto {
    pub id: Uuid,
    pub name: String,
    pub gender: String,
    pub gender_probability: f64,
    pub sample_size: i32,
    pub age: i32,
    pub age_group: String,
    pub country_id: String,
    pub country_probability: f64,
    pub created_at: DateTime<Utc>,
}

impl From<Profile> for ProfileDetailDto {
    fn from(p: Profile) -> Self {
        Self {
            id: p.id,
            name: p.name,
            gender: p.gender,
            gender_probability: p.gender_probability,
            sample_size: p.sample_size,
            age: p.age,
            age_group: p.age_group,
            country_id: p.country_id,
            country_probability: p.country_probability,
            created_at: p.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ProfileListItemDto {
    pub id: Uuid,
    pub name: String,
    pub gender: String,
    pub age: i32,
    pub age_group: String,
    pub country_id: String,
}

impl From<Profile> for ProfileListItemDto {
    fn from(p: Profile) -> Self {
        Self {
            id: p.id,
            name: p.name,
            gender: p.gender,
            age: p.age,
            age_group: p.age_group,
            country_id: p.country_id,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct GenderizeResponse {
    #[allow(dead_code)]
    pub name: String,
    pub gender: Option<String>,
    pub probability: f64,
    pub count: i64,
}

#[derive(Debug, Deserialize)]
pub struct AgifyResponse {
    #[allow(dead_code)]
    pub name: String,
    pub age: Option<i64>,
    #[allow(dead_code)]
    pub count: i64,
}

#[derive(Debug, Deserialize)]
pub struct NationalizeCountry {
    pub country_id: String,
    pub probability: f64,
}

#[derive(Debug, Deserialize)]
pub struct NationalizeResponse {
    #[allow(dead_code)]
    pub name: String,
    pub country: Vec<NationalizeCountry>,
}

#[derive(Debug, Deserialize)]
pub struct ListProfilesQuery {
    pub gender: Option<String>,
    pub country_id: Option<String>,
    pub age_group: Option<String>,
}
