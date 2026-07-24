//! PostgreSQL connection pool and readiness primitives.

mod config;
mod postgres;

pub use config::{DatabaseConfig, DatabaseConfigError};
pub use postgres::{connect, ensure_initialized, ping};
pub use sqlx::{PgPool, Postgres, Transaction};
