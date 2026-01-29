pub mod handlers;
pub mod import_pointcloud;
pub mod import_raster;

pub use handlers::*;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Input value - either inline base64 data or a reference URL
/// This follows OGC API Processes specification for binary inputs.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum InputValue {
    /// Inline binary data (base64-encoded)
    Inline(InlineValue),
    /// Reference to external file
    Reference(ReferenceValue),
}

/// Inline base64-encoded binary value (OGC API Processes qualifiedValue)
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InlineValue {
    /// Base64-encoded file content
    pub value: String,
    /// Media type of the data
    #[serde(default)]
    pub media_type: Option<String>,
}

/// Reference to external file (href)
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ReferenceValue {
    /// URL to the file (S3 URI or HTTP URL)
    pub href: String,
    /// Media type of the referenced file
    #[serde(rename = "type")]
    #[serde(default)]
    pub media_type: Option<String>,
}
