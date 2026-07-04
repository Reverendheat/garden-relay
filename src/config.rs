use std::{env, net::SocketAddr};

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub openai_base_url: String,
    pub database_path: String,
    pub lifecycle_store_capacity: usize,
    pub policy_dir: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let host = env::var("GARDEN_RELAY_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned());
        let port = env::var("GARDEN_RELAY_PORT")
            .ok()
            .map(|value| value.parse())
            .transpose()?
            .unwrap_or(8080);
        let openai_base_url =
            env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com".to_owned());
        let database_path =
            env::var("GARDEN_RELAY_DATABASE_PATH").unwrap_or_else(|_| "gardenrelay.db".to_owned());
        let lifecycle_store_capacity = env::var("GARDEN_RELAY_LIFECYCLE_STORE_CAPACITY")
            .ok()
            .map(|value| value.parse())
            .transpose()?
            .unwrap_or(1_000);
        let policy_dir =
            env::var("GARDEN_RELAY_POLICY_DIR").unwrap_or_else(|_| "policies".to_owned());

        Ok(Self {
            host,
            port,
            openai_base_url,
            database_path,
            lifecycle_store_capacity,
            policy_dir,
        })
    }

    pub fn bind_addr(&self) -> anyhow::Result<SocketAddr> {
        Ok(format!("{}:{}", self.host, self.port).parse()?)
    }
}
