use schemars::JsonSchema;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number_returned: Option<u64>,
}

/// Request to create a new collection
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateCollectionRequest {
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
