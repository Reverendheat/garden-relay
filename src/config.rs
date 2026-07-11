use std::{env, net::SocketAddr};

use crate::auth::AuthMode;

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub openai_base_url: String,
    pub database_path: String,
    pub lifecycle_store_capacity: usize,
    pub policy_dir: String,
    pub auth_mode: AuthMode,
    pub bootstrap_token: Option<String>,
    pub operator_session_ttl_seconds: i64,
    pub session_cookie_secure: bool,
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
        let auth_mode = env::var("GARDEN_RELAY_AUTH_MODE")
            .unwrap_or_else(|_| "disabled".to_owned())
            .parse()?;
        let bootstrap_token = env::var("GARDEN_RELAY_BOOTSTRAP_TOKEN")
            .ok()
            .filter(|token| !token.is_empty());
        let operator_session_ttl_seconds = env::var("GARDEN_RELAY_OPERATOR_SESSION_TTL_SECONDS")
            .ok()
            .map(|value| value.parse())
            .transpose()?
            .unwrap_or(8 * 60 * 60);
        let session_cookie_secure = env::var("GARDEN_RELAY_SESSION_COOKIE_SECURE")
            .ok()
            .map(|value| value.parse())
            .transpose()?
            .unwrap_or(false);

        Ok(Self {
            host,
            port,
            openai_base_url,
            database_path,
            lifecycle_store_capacity,
            policy_dir,
            auth_mode,
            bootstrap_token,
            operator_session_ttl_seconds,
            session_cookie_secure,
        })
    }

    pub fn bind_addr(&self) -> anyhow::Result<SocketAddr> {
        Ok(format!("{}:{}", self.host, self.port).parse()?)
    }
}
