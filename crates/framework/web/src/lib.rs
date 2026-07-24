//! Shared HTTP conventions used by every service and business module.

mod error;
mod middleware;

pub use error::{AppError, ErrorBody};
pub use kairos_platform_framework_common::{ApiResponse, HealthResponse, health_route, serve};
pub use middleware::{WebConfig, apply_web_layers};
