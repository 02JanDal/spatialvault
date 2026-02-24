use aide::OperationInput;
use aide::generate::GenContext;
use aide::openapi::{MediaType, RequestBody, SchemaObject};
use axum::Json;
use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use schemars::{JsonSchema, json_schema};
use serde::{Deserialize, Serialize};

use crate::api::common::{
    Extent, Link, UploadedFile, is_json, is_multipart, parse_multipart_with_files,
};

/// Supported column types for collection properties
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ColumnType {
    String,
    Integer,
    Real,
    Date,
    Datetime,
    Boolean,
}

/// Column definition for collection properties
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ColumnDef {
    pub name: String,
    #[serde(rename = "type")]
    pub column_type: ColumnType,
    #[serde(default = "default_true")]
    pub nullable: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
}

fn default_true() -> bool {
    true
}

/// OGC API / STAC Collection response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CollectionResponse {
    #[serde(rename = "type")]
    pub collection_type: String,
    pub stac_version: String,
    pub stac_extensions: Vec<String>,
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
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
    pub number_matched: u64,
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
    /// Column definitions for feature properties
    #[serde(default)]
    pub columns: Option<Vec<ColumnDef>>,
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
        if is_multipart(&req) {
            let (metadata, mut files) =
                parse_multipart_with_files::<S, CreateCollection>(req, state, "metadata").await?;

            let file = files.remove("file").ok_or_else(|| {
                axum::response::Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body("Missing 'file' field".into())
                    .unwrap()
            })?;

            if !files.is_empty() {
                return Err(axum::response::Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body("Unexpected form fields".into())
                    .unwrap());
            }

            Ok(CreateCollectionRequest::Multipart { metadata, file })
        } else if is_json(&req) {
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
    /// Columns to add to the collection
    #[serde(default)]
    pub add_columns: Option<Vec<ColumnDef>>,
    /// Column names to remove from the collection
    #[serde(default)]
    pub remove_columns: Option<Vec<String>>,
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
