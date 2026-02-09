use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Process definition for vector import
pub const PROCESS_ID: &str = "import-vector";

/// Input schema for vector import (internal job inputs)
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportVectorInputs {
    /// Target collection ID
    pub collection_id: String,

    /// S3 key where the uploaded file is stored
    pub file_key: String,
}

/// Output schema for vector import
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportVectorOutputs {
    /// Collection the features were imported to
    pub collection: String,

    /// Number of features successfully imported
    pub features_imported: i64,

    /// Number of features that failed to import
    pub features_failed: i64,

    /// Source CRS (EPSG code)
    pub source_crs: String,

    /// Target storage CRS (EPSG code)
    pub target_crs: String,
}
