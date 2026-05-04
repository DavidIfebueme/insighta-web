use anyhow::Context;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub database_max_connections: u32,
    pub server_addr: String,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .context("DATABASE_URL must be set")?;
        let database_max_connections = std::env::var("DATABASE_MAX_CONNECTIONS")
            .unwrap_or_else(|_| "5".to_string())
            .parse()?;
        let server_addr = std::env::var("SERVER_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:3000".to_string());

        Ok(Self {
            database_url,
            database_max_connections,
            server_addr,
        })
    }
}
