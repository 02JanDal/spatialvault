use aide::OperationInput;
use aide::generate::GenContext;
use aide::openapi::{MediaType, RequestBody, SchemaObject};
use axum::Json;
use axum::extract::{FromRequest, Multipart, Request};
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::IntoResponse;
use bytes::Bytes;
use schemars::{JsonSchema, json_schema};
use serde::{Deserialize, Serialize};

use crate::api::common::{Extent, Link};

/// OGC API Collection response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CollectionResponse {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub links: Vec<Link>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extent: Option<Extent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_crs: Option<String>,
}

/// List of collections
#[derive(Debug, Serialize, JsonSchema)]
pub struct CollectionsResponse {
    pub collections: Vec<CollectionResponse>,
    pub links: Vec<Link>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number_matched: Option<u64>,
    pub number_returned: u64,
}

/// Request to create a new collection
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateCollection {
    /// Colon-separated hierarchical name (e.g., "folder:subfolder:collection")
    /// First segment is always the owner's username (auto-prepended if missing)
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Collection type: "vector", "raster", or "pointcloud"
    pub collection_type: String,
    /// Optional owner override (for group ownership)
    #[serde(default)]
    pub owner: Option<String>,
    /// CRS for the collection (EPSG code). Default: 4326
    #[serde(default = "default_crs")]
    pub crs: i32,
}

#[derive(Debug)]
pub struct UploadedFile {
    pub filename: String,
    pub content_type: String,
    pub data: Bytes,
}

#[derive(Debug)]
pub enum CreateCollectionRequest {
    Json(CreateCollection),
    Multipart {
        metadata: CreateCollection,
        file: UploadedFile,
    },
}

impl<S> FromRequest<S> for CreateCollectionRequest
where
    S: Send + Sync,
{
    type Rejection = axum::response::Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let content_type = req
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if content_type.starts_with("multipart/form-data") {
            let mut multipart = Multipart::from_request(req, state)
                .await
                .map_err(|e| e.into_response())?;

            let mut metadata: Option<CreateCollection> = None;
            let mut file: Option<UploadedFile> = None;

            while let Some(field) = multipart
                .next_field()
                .await
                .map_err(|e| e.into_response())?
            {
                match field.name() {
                    Some("metadata") => {
                        let bytes = field.bytes().await.map_err(|e| e.into_response())?;
                        metadata = Some(serde_json::from_slice(&bytes).map_err(|e| {
                            axum::response::Response::builder()
                                .status(StatusCode::BAD_REQUEST)
                                .body(format!("Invalid metadata JSON: {}", e).into())
                                .unwrap()
                        })?);
                    }
                    Some("file") => {
                        let filename = field.file_name().unwrap_or("file").to_string();
                        let content_type = field
                            .content_type()
                            .unwrap_or("application/octet-stream")
                            .to_string();
                        let data = field.bytes().await.map_err(|e| e.into_response())?;
                        file = Some(UploadedFile {
                            filename,
                            content_type,
                            data,
                        });
                    }
                    _ => {
                        return Err(axum::response::Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body("Unexpected form field".into())
                            .unwrap());
                    }
                }
            }

            let metadata = metadata.ok_or_else(|| {
                axum::response::Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body("Missing metadata field".into())
                    .unwrap()
            })?;
            let file = file.ok_or_else(|| {
                axum::response::Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body("Missing file field".into())
                    .unwrap()
            })?;

            Ok(CreateCollectionRequest::Multipart { metadata, file })
        } else if content_type.starts_with("application/json") {
            let metadata = Json::<CreateCollection>::from_request(req, state)
                .await
                .map_err(|e| e.into_response())?
                .0;
            Ok(CreateCollectionRequest::Json(metadata))
        } else {
            Err(axum::response::Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body("Unsupported Content-Type".into())
                .unwrap())
        }
    }
}
impl OperationInput for CreateCollectionRequest {
    fn operation_input(ctx: &mut GenContext, operation: &mut aide::openapi::Operation) {
        let json_schema = ctx.schema.subschema_for::<CreateCollection>();

        // Multipart schema: an object with a "metadata" (JSON) part and a "file" (binary) part
        let multipart_schema = json_schema!({
            "type": "object",
             "properties": {
                "metadata": ctx.schema.subschema_for::<CreateCollection>(),
                "file": {
                    "type": "string",
                    "format": "binary"
                }
             },
             "required": ["metadata", "file"]
        });

        let request_body = RequestBody {
            description: Some(
                "JSON body, or multipart with a JSON 'metadata' part and a 'file' part".into(),
            ),
            content: indexmap::indexmap! {
                "application/json".to_string() => MediaType {
                    schema: Some(SchemaObject {
                        json_schema: json_schema,
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
                    // This tells OpenAPI that the "metadata" part is JSON-encoded
                    encoding: indexmap::indexmap! {
                        "metadata".to_string() => aide::openapi::Encoding {
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

fn default_crs() -> i32 {
    4326
}

/// Request to update a collection
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCollectionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// New canonical name for rename/move (creates alias from old name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// Collection schema (OGC API Schemas)
#[derive(Debug, Serialize, JsonSchema)]
pub struct CollectionSchema {
    #[serde(rename = "$schema")]
    pub schema: String,
    #[serde(rename = "$id")]
    pub id: String,
    #[serde(rename = "type")]
    pub schema_type: String,
    pub title: String,
    pub properties: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

/// Query parameters for listing collections
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListCollectionsParams {
    /// Filter by collection type
    #[serde(rename = "type")]
    pub collection_type: Option<String>,
    /// Filter by owner
    pub owner: Option<String>,
    /// Limit results
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Offset for pagination
    #[serde(default)]
    pub offset: u32,
}

fn default_limit() -> u32 {
    100
}
