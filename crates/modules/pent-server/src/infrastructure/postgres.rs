//! PostgreSQL repository — stub for Phase 1.

use kairos_platform_framework_database::PgPool;

pub struct PentRepository {
    pool: PgPool,
}

impl PentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
