use aide::OperationOutput;
use aide::openapi::{MediaType, Response as AideResponse};
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Precondition failed: {0}")]
    PreconditionFailed(String),

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Processing error: {0}")]
    Processing(String),
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ErrorResponse {
    pub code: String,
    pub description: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, description) = match &self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "NotFound", msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BadRequest", msg.clone()),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, "Unauthorized", msg.clone()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, "Forbidden", msg.clone()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, "Conflict", msg.clone()),
            AppError::PreconditionFailed(msg) => (
                StatusCode::PRECONDITION_FAILED,
                "PreconditionFailed",
                msg.clone(),
            ),
            AppError::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "InternalError",
                    "An internal error occurred".to_string(),
                )
            }
            AppError::Database(e) => {
                tracing::error!("Database error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "DatabaseError",
                    "Database error occurred".to_string(),
                )
            }
            AppError::Serialization(e) => {
                tracing::error!("Serialization error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "SerializationError",
                    "Serialization error occurred".to_string(),
                )
            }
            AppError::Io(e) => {
                tracing::error!("IO error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "IoError",
                    "IO error occurred".to_string(),
                )
            }
            AppError::Config(msg) => {
                tracing::error!("Config error: {}", msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "ConfigError",
                    "Configuration error".to_string(),
                )
            }
            AppError::Storage(msg) => {
                tracing::error!("Storage error: {}", msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "StorageError",
                    "A storage error occurred".to_string(),
                )
            }
            AppError::Processing(msg) => {
                tracing::error!("Processing error: {}", msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "ProcessingError",
                    "A processing error occurred".to_string(),
                )
            }
        };

        let body = Json(ErrorResponse {
            code: code.to_string(),
            description,
        });

        (status, body).into_response()
    }
}

impl OperationOutput for AppError {
    type Inner = ErrorResponse;

    fn operation_response(
        ctx: &mut aide::generate::GenContext,
        _operation: &mut aide::openapi::Operation,
    ) -> Option<AideResponse> {
        let schema = ctx.schema.subschema_for::<ErrorResponse>();

        let mut content = IndexMap::new();
        content.insert(
            "application/json".to_string(),
            MediaType {
                schema: Some(aide::openapi::SchemaObject {
                    json_schema: schema,
                    external_docs: None,
                    example: None,
                }),
                ..Default::default()
            },
        );

        Some(AideResponse {
            description: "Error response".to_string(),
            content,
            ..Default::default()
        })
    }
}

pub type AppResult<T> = Result<T, AppError>;
