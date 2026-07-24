use std::{env, fmt, time::Duration};

#[derive(Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub min_connections: u32,
    pub max_connections: u32,
    pub acquire_timeout: Duration,
}

impl DatabaseConfig {
    pub fn from_env() -> Result<Self, DatabaseConfigError> {
        let url = env::var("DATABASE_URL").map_err(|_| DatabaseConfigError::MissingUrl)?;
        let min_connections = parse_env("DATABASE_MIN_CONNECTIONS", 1)?;
        let max_connections = parse_env("DATABASE_MAX_CONNECTIONS", 20)?;
        let acquire_timeout_seconds = parse_env("DATABASE_ACQUIRE_TIMEOUT_SECONDS", 5)?;

        Self::new(
            url,
            min_connections,
            max_connections,
            Duration::from_secs(acquire_timeout_seconds.into()),
        )
    }

    pub fn new(
        url: impl Into<String>,
        min_connections: u32,
        max_connections: u32,
        acquire_timeout: Duration,
    ) -> Result<Self, DatabaseConfigError> {
        let url = url.into();
        if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
            return Err(DatabaseConfigError::InvalidUrl);
        }
        if max_connections == 0 || min_connections > max_connections {
            return Err(DatabaseConfigError::InvalidPoolSize);
        }
        if acquire_timeout.is_zero() {
            return Err(DatabaseConfigError::InvalidTimeout);
        }

        Ok(Self {
            url,
            min_connections,
            max_connections,
            acquire_timeout,
        })
    }
}

impl fmt::Debug for DatabaseConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DatabaseConfig")
            .field("url", &"[REDACTED]")
            .field("min_connections", &self.min_connections)
            .field("max_connections", &self.max_connections)
            .field("acquire_timeout", &self.acquire_timeout)
            .finish()
    }
}

fn parse_env(name: &'static str, default: u32) -> Result<u32, DatabaseConfigError> {
    env::var(name)
        .ok()
        .map(|value| value.parse())
        .transpose()
        .map_err(|_| DatabaseConfigError::InvalidNumber(name))
        .map(|value| value.unwrap_or(default))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseConfigError {
    MissingUrl,
    InvalidUrl,
    InvalidPoolSize,
    InvalidTimeout,
    InvalidNumber(&'static str),
}

impl fmt::Display for DatabaseConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingUrl => formatter.write_str("DATABASE_URL is required"),
            Self::InvalidUrl => formatter.write_str("DATABASE_URL must be a PostgreSQL URL"),
            Self::InvalidPoolSize => formatter.write_str("database pool size is invalid"),
            Self::InvalidTimeout => {
                formatter.write_str("database acquire timeout must be positive")
            }
            Self::InvalidNumber(name) => write!(formatter, "{name} must be a positive integer"),
        }
    }
}

impl std::error::Error for DatabaseConfigError {}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{DatabaseConfig, DatabaseConfigError};

    #[test]
    fn validates_pool_bounds() {
        let result = DatabaseConfig::new(
            "postgres://localhost/kairos_platform",
            10,
            5,
            Duration::from_secs(1),
        );
        assert!(matches!(result, Err(DatabaseConfigError::InvalidPoolSize)));
    }

    #[test]
    fn redacts_database_url_in_debug_output() {
        let config = DatabaseConfig::new(
            "postgres://user:secret@localhost/kairos_platform",
            1,
            5,
            Duration::from_secs(1),
        )
        .unwrap();
        let output = format!("{config:?}");

        assert!(!output.contains("secret"));
        assert!(output.contains("REDACTED"));
    }
}
