use axum::{Json, http::StatusCode, response::IntoResponse};
use kairos_platform_framework_common::ApiResponse;
use serde::Serialize;

pub type ErrorBody = ApiResponse<()>;

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    code: u16,
    message: String,
}

impl AppError {
    pub fn new(status: StatusCode, code: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, 400, message)
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, 401, message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, 403, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, 404, message)
    }

    pub fn not_implemented(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_IMPLEMENTED, 501, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, 500, message)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let body = ErrorBody {
            code: self.code,
            data: (),
            message: self.message,
        };
        (self.status, Json(body)).into_response()
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        ErrorBody {
            code: self.code,
            data: (),
            message: self.message.clone(),
        }
        .serialize(serializer)
    }
}
