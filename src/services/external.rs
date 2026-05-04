use crate::errors::AppError;
use crate::models::{AgifyResponse, GenderizeResponse, NationalizeResponse};

pub struct ExternalApiService {
    client: reqwest::Client,
}

impl ExternalApiService {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    pub async fn fetch_genderize(
        &self,
        name: &str,
    ) -> Result<GenderizeResponse, AppError> {
        let resp = self.client.get("https://api.genderize.io")
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

    pub async fn fetch_agify(&self, name: &str) -> Result<AgifyResponse, AppError> {
        let resp = self.client.get("https://api.agify.io")
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

    pub async fn fetch_nationalize(
        &self,
        name: &str,
    ) -> Result<NationalizeResponse, AppError> {
        let resp = self.client.get("https://api.nationalize.io")
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
}
