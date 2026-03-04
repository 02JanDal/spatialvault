use aide::OperationInput;
use aide::generate::GenContext;
use aide::openapi::{MediaType, RequestBody, SchemaObject};
use axum::Json;
use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use schemars::json_schema;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use super::handlers::{CreateFeatureRequest, UpdateFeatureRequest};
use crate::api::common::{Assets, UploadedFile, is_json, is_multipart, parse_multipart_with_files};
use crate::error::{AppError, BadRequest, Storage};
use crate::storage::S3Storage;

// ---------------------------------------------------------------------------
// Payload enums
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum CreateFeaturePayload {
    Json(CreateFeatureRequest),
    Multipart {
        item: CreateFeatureRequest,
        files: HashMap<String, UploadedFile>,
    },
}

#[derive(Debug)]
pub enum UpdateFeaturePayload {
    Json(UpdateFeatureRequest),
    Multipart {
        item: UpdateFeatureRequest,
        files: HashMap<String, UploadedFile>,
    },
}

// ---------------------------------------------------------------------------
// FromRequest impls
// ---------------------------------------------------------------------------

impl<S> FromRequest<S> for CreateFeaturePayload
where
    S: Send + Sync,
{
    type Rejection = axum::response::Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        if is_multipart(&req) {
            let (item, files) =
                parse_multipart_with_files::<S, CreateFeatureRequest>(req, state, "item").await?;
            Ok(CreateFeaturePayload::Multipart { item, files })
        } else if is_json(&req) {
            let item = Json::<CreateFeatureRequest>::from_request(req, state)
                .await
                .map_err(|e| e.into_response())?
                .0;
            Ok(CreateFeaturePayload::Json(item))
        } else {
            Err(axum::response::Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body("Unsupported Content-Type".into())
                .unwrap())
        }
    }
}

impl<S> FromRequest<S> for UpdateFeaturePayload
where
    S: Send + Sync,
{
    type Rejection = axum::response::Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        if is_multipart(&req) {
            let (item, files) =
                parse_multipart_with_files::<S, UpdateFeatureRequest>(req, state, "item").await?;
            Ok(UpdateFeaturePayload::Multipart { item, files })
        } else if is_json(&req) {
            let item = Json::<UpdateFeatureRequest>::from_request(req, state)
                .await
                .map_err(|e| e.into_response())?
                .0;
            Ok(UpdateFeaturePayload::Json(item))
        } else {
            Err(axum::response::Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body("Unsupported Content-Type".into())
                .unwrap())
        }
    }
}

// ---------------------------------------------------------------------------
// OperationInput impls (OpenAPI docs)
// ---------------------------------------------------------------------------

impl OperationInput for CreateFeaturePayload {
    fn operation_input(ctx: &mut GenContext, operation: &mut aide::openapi::Operation) {
        let json_schema = ctx.schema.subschema_for::<CreateFeatureRequest>();

        let multipart_schema = json_schema!({
            "type": "object",
            "properties": {
                "item": ctx.schema.subschema_for::<CreateFeatureRequest>(),
            },
            "additionalProperties": {
                "type": "string",
                "format": "binary"
            },
            "required": ["item"]
        });

        let request_body = RequestBody {
            description: Some(
                "JSON body, or multipart with a JSON 'item' part and file parts for asset uploads"
                    .into(),
            ),
            content: indexmap::indexmap! {
                "application/json".to_string() => MediaType {
                    schema: Some(SchemaObject {
                        json_schema,
                        external_docs: None,
                        example: None,
                    }),
                    ..Default::default()
                },
                "multipart/form-data".to_string() => MediaType {
                    schema: Some(SchemaObject {
                        json_schema: multipart_schema.into(),
                        external_docs: None,
                        example: None,
                    }),
                    encoding: indexmap::indexmap! {
                        "item".to_string() => aide::openapi::Encoding {
                            content_type: Some("application/json".to_string()),
                            ..Default::default()
                        },
                    },
                    ..Default::default()
                },
            },
            required: true,
            ..Default::default()
        };

        operation.request_body = Some(aide::openapi::ReferenceOr::Item(request_body));
    }
}

impl OperationInput for UpdateFeaturePayload {
    fn operation_input(ctx: &mut GenContext, operation: &mut aide::openapi::Operation) {
        let json_schema = ctx.schema.subschema_for::<UpdateFeatureRequest>();

        let multipart_schema = json_schema!({
            "type": "object",
            "properties": {
                "item": ctx.schema.subschema_for::<UpdateFeatureRequest>(),
            },
            "additionalProperties": {
                "type": "string",
                "format": "binary"
            },
            "required": ["item"]
        });

        let request_body = RequestBody {
            description: Some(
                "JSON body, or multipart with a JSON 'item' part and file parts for asset uploads"
                    .into(),
            ),
            content: indexmap::indexmap! {
                "application/json".to_string() => MediaType {
                    schema: Some(SchemaObject {
                        json_schema,
                        external_docs: None,
                        example: None,
                    }),
                    ..Default::default()
                },
                "multipart/form-data".to_string() => MediaType {
                    schema: Some(SchemaObject {
                        json_schema: multipart_schema.into(),
                        external_docs: None,
                        example: None,
                    }),
                    encoding: indexmap::indexmap! {
                        "item".to_string() => aide::openapi::Encoding {
                            content_type: Some("application/json".to_string()),
                            ..Default::default()
                        },
                    },
                    ..Default::default()
                },
            },
            required: true,
            ..Default::default()
        };

        operation.request_body = Some(aide::openapi::ReferenceOr::Item(request_body));
    }
}

// ---------------------------------------------------------------------------
// resolve_asset_uploads
// ---------------------------------------------------------------------------

/// Scan asset hrefs for `@name` references, validate against uploaded files,
/// upload files to S3, and rewrite the hrefs to S3 URIs.
pub async fn resolve_asset_uploads(
    assets: &mut Option<Assets>,
    files: HashMap<String, UploadedFile>,
    storage: Option<&S3Storage>,
) -> Result<(), AppError> {
    let assets_map = match assets {
        Some(m) if !m.is_empty() => m,
        _ => {
            // No assets declared — but there should be no files either
            if !files.is_empty() {
                let extra: Vec<_> = files.keys().cloned().collect();
                return Err(BadRequest {
                    message: format!(
                        "Uploaded file(s) not referenced by any asset: {}",
                        extra.join(", ")
                    ),
                }
                .build());
            }
            return Ok(());
        }
    };

    // Collect @-mentions from asset hrefs
    let mut mentions: HashSet<String> = HashSet::new();
    for asset in assets_map.values() {
        if let Some(name) = asset.href.strip_prefix('@') {
            mentions.insert(name.to_string());
        }
    }

    // If no @-mentions, files must also be empty
    if mentions.is_empty() {
        if !files.is_empty() {
            let extra: Vec<_> = files.keys().cloned().collect();
            return Err(BadRequest {
                message: format!(
                    "Uploaded file(s) not referenced by any asset: {}",
                    extra.join(", ")
                ),
            }
            .build());
        }
        return Ok(());
    }

    // @-mentions require storage
    let storage = storage.ok_or_else(|| {
        BadRequest {
            message: "Asset file uploads require S3 storage to be configured".to_string(),
        }
        .build()
    })?;

    // Validate 1:1 mapping between @-mentions and file parts
    let file_names: HashSet<&String> = files.keys().collect();
    let mention_refs: HashSet<&String> = mentions.iter().collect();

    let missing: Vec<_> = mention_refs
        .difference(&file_names)
        .map(|s| (*s).clone())
        .collect();
    if !missing.is_empty() {
        return Err(BadRequest {
            message: format!(
                "Asset(s) reference files not included in upload: {}",
                missing.join(", ")
            ),
        }
        .build());
    }

    let extra: Vec<_> = file_names
        .difference(&mention_refs)
        .map(|s| (*s).clone())
        .collect();
    if !extra.is_empty() {
        return Err(BadRequest {
            message: format!(
                "Uploaded file(s) not referenced by any asset: {}",
                extra.join(", ")
            ),
        }
        .build());
    }

    // Upload each referenced file and rewrite hrefs
    for asset in assets_map.values_mut() {
        if let Some(name) = asset.href.strip_prefix('@') {
            let name = name.to_string();
            if let Some(file) = files.get(&name) {
                let key = format!("{}-{}", Uuid::new_v4(), file.filename);
                storage.put(&key, file.data.clone()).await.map_err(|e| {
                    Storage {
                        message: format!("Failed to upload asset '{}': {}", name, e),
                    }
                    .build()
                })?;

                asset.href = storage.s3_uri(&key);

                // Auto-fill media_type and file_size if not set
                if asset.media_type.is_none() {
                    asset.media_type = Some(file.content_type.clone());
                }
                if asset.file_size.is_none() {
                    asset.file_size = Some(file.data.len() as i64);
                }
            }
        }
    }

    Ok(())
}
