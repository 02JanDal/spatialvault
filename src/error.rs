use aide::OperationOutput;
use aide::openapi::{MediaType, Response as AideResponse};
use axum::http::{HeaderMap, header};
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::Serialize;
use snafu::prelude::*;
use std::backtrace::Backtrace;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(false)), visibility(pub))]
pub enum AppError {
    #[snafu(display("Not found: {message}"))]
    NotFound {
        message: String,
        backtrace: Backtrace,
    },

    #[snafu(display("Bad request: {message}"))]
    BadRequest {
        message: String,
        backtrace: Backtrace,
    },

    #[snafu(display("Unauthorized: {message}"))]
    Unauthorized {
        message: String,
        backtrace: Backtrace,
    },

    #[snafu(display("Forbidden: {message}"))]
    Forbidden {
        message: String,
        backtrace: Backtrace,
    },

    #[snafu(display("Conflict: {message}"))]
    Conflict {
        message: String,
        backtrace: Backtrace,
    },

    #[snafu(display("Precondition failed: {message}"))]
    PreconditionFailed {
        message: String,
        backtrace: Backtrace,
    },

    #[snafu(display("Internal server error: {message}"))]
    Internal {
        message: String,
        backtrace: Backtrace,
    },

    #[snafu(display("Database error: {source:?}"))]
    #[snafu(context(false))]
    Database {
        source: sqlx::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("Serialization error: {source}"))]
    #[snafu(context(false))]
    Serialization {
        source: serde_json::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("IO error: {source}"))]
    #[snafu(context(false))]
    Io {
        source: std::io::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("Configuration error: {message}"))]
    Config {
        message: String,
        backtrace: Backtrace,
    },

    #[snafu(display("Storage error: {message}"))]
    Storage {
        message: String,
        backtrace: Backtrace,
    },

    #[snafu(display("Processing error: {message}"))]
    Processing {
        message: String,
        backtrace: Backtrace,
    },

    #[snafu(display("Collection renamed"))]
    RenamedTo {
        message: String,
        backtrace: Backtrace,
    },
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ErrorResponse {
    pub code: String,
    pub description: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, description) = match &self {
            AppError::NotFound { message, .. } => {
                (StatusCode::NOT_FOUND, "NotFound", message.clone())
            }
            AppError::BadRequest { message, .. } => {
                (StatusCode::BAD_REQUEST, "BadRequest", message.clone())
            }
            AppError::Unauthorized { message, .. } => {
                (StatusCode::UNAUTHORIZED, "Unauthorized", message.clone())
            }
            AppError::Forbidden { message, .. } => {
                (StatusCode::FORBIDDEN, "Forbidden", message.clone())
            }
            AppError::Conflict { message, .. } => {
                (StatusCode::CONFLICT, "Conflict", message.clone())
            }
            AppError::PreconditionFailed { message, .. } => (
                StatusCode::PRECONDITION_FAILED,
                "PreconditionFailed",
                message.clone(),
            ),
            AppError::Internal { message, .. } => {
                tracing::error!("Internal error: {}", message);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "InternalError",
                    "An internal error occurred".to_string(),
                )
            }
            AppError::Database { source, .. } => {
                // Check for PostgreSQL permission errors (42501 = insufficient_privilege)
                if let sqlx::Error::Database(db_err) = &source {
                    if db_err.code().as_deref() == Some("42501") {
                        tracing::info!("Database permission error: {}", db_err);
                        return (
                            StatusCode::FORBIDDEN,
                            Json(ErrorResponse {
                                code: "Forbidden".to_string(),
                                description: "Permission denied".to_string(),
                            }),
                        )
                            .into_response();
                    }
                }
                tracing::error!("Database error: {}", source);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "DatabaseError",
                    "Database error occurred".to_string(),
                )
            }
            AppError::Serialization { source, .. } => {
                tracing::error!("Serialization error: {}", source);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "SerializationError",
                    "Serialization error occurred".to_string(),
                )
            }
            AppError::Io { source, .. } => {
                tracing::error!("IO error: {}", source);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "IoError",
                    "IO error occurred".to_string(),
                )
            }
            AppError::Config { message, .. } => {
                tracing::error!("Config error: {}", message);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "ConfigError",
                    "Configuration error".to_string(),
                )
            }
            AppError::Storage { message, .. } => {
                tracing::error!("Storage error: {}", message);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "StorageError",
                    "A storage error occurred".to_string(),
                )
            }
            AppError::Processing { message, .. } => {
                tracing::error!("Processing error: {}", message);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "ProcessingError",
                    "A processing error occurred".to_string(),
                )
            }
            AppError::RenamedTo { message, .. } => {
                let mut headers = HeaderMap::new();
                headers.insert(header::LOCATION, message.parse().unwrap());
                return (StatusCode::TEMPORARY_REDIRECT, headers).into_response();
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
