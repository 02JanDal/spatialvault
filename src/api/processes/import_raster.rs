use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

// Re-export common types for convenience
pub use super::{InlineValue, InputValue, ReferenceValue};

/// Process definition for raster import
pub const PROCESS_ID: &str = "import-raster";

/// Input schema for raster import
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportRasterInputs {
    /// Target collection ID (creates if doesn't exist)
    pub collection: String,

    /// Raster data - either inline (base64) or reference (href)
    pub data: InputValue,

    /// Optional item title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Optional datetime for the item
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datetime: Option<String>,

    /// Additional properties for the item
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<serde_json::Value>,

    /// Whether to skip conversion if already COG
    #[serde(default = "default_skip_if_cog")]
    pub skip_if_cog: bool,
}

fn default_skip_if_cog() -> bool {
    true
}

impl ImportRasterInputs {
    /// Validate the inputs
    pub fn validate(&self) -> AppResult<()> {
        // Validate collection name
        if self.collection.is_empty() {
            return Err(AppError::BadRequest(
                "collection is required".to_string(),
            ));
        }

        // Validate data input
        match &self.data {
            InputValue::Inline(inline) => {
                if inline.value.is_empty() {
                    return Err(AppError::BadRequest(
                        "data.value cannot be empty".to_string(),
                    ));
                }
                // Validate base64
                if base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    &inline.value,
                )
                .is_err()
                {
                    return Err(AppError::BadRequest(
                        "data.value must be valid base64".to_string(),
                    ));
                }
            }
            InputValue::Reference(reference) => {
                if reference.href.is_empty() {
                    return Err(AppError::BadRequest(
                        "data.href cannot be empty".to_string(),
                    ));
                }
                // Validate URL scheme
                if !reference.href.starts_with("s3://")
                    && !reference.href.starts_with("http://")
                    && !reference.href.starts_with("https://")
                {
                    return Err(AppError::BadRequest(
                        "data.href must be an S3 URI or HTTP(S) URL".to_string(),
                    ));
                }
            }
        }

        // Validate datetime if provided
        if let Some(ref dt) = self.datetime {
            if chrono::DateTime::parse_from_rfc3339(dt).is_err() {
                return Err(AppError::BadRequest(
                    "datetime must be a valid RFC3339 timestamp".to_string(),
                ));
            }
        }

        Ok(())
    }
}

/// Output schema for raster import
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportRasterOutputs {
    /// Created item ID
    pub item_id: String,

    /// Collection the item was added to
    pub collection: String,

    /// Asset href (S3 URI)
    pub asset_href: String,

    /// Whether conversion was performed
    pub converted: bool,
}

/// Process description for OpenAPI
pub fn process_description() -> serde_json::Value {
    serde_json::json!({
        "id": PROCESS_ID,
        "title": "Import Raster",
        "description": "Import a raster file into a collection. Accepts COG (pass-through) or other formats (converted to COG via GDAL). Data can be provided inline (base64-encoded) or as a reference URL.",
        "version": "1.0.0",
        "jobControlOptions": ["async-execute"],
        "outputTransmission": ["value"],
        "inputs": {
            "collection": {
                "title": "Collection ID",
                "description": "Target collection ID. Collection will be created if it doesn't exist.",
                "schema": { "type": "string", "minLength": 1 }
            },
            "data": {
                "title": "Raster Data",
                "description": "Raster file data - either inline base64-encoded content or a reference URL",
                "schema": {
                    "oneOf": [
                        {
                            "type": "object",
                            "title": "Inline Value",
                            "required": ["value"],
                            "properties": {
                                "value": {
                                    "type": "string",
                                    "contentEncoding": "base64",
                                    "description": "Base64-encoded raster file content"
                                },
                                "mediaType": {
                                    "type": "string",
                                    "description": "Media type (e.g., image/tiff)"
                                }
                            }
                        },
                        {
                            "type": "object",
                            "title": "Reference Value",
                            "required": ["href"],
                            "properties": {
                                "href": {
                                    "type": "string",
                                    "format": "uri",
                                    "description": "URL to the raster file (S3 URI or HTTP URL)"
                                },
                                "type": {
                                    "type": "string",
                                    "description": "Media type of the referenced file"
                                }
                            }
                        }
                    ]
                }
            },
            "title": {
                "title": "Item Title",
                "description": "Optional title for the item",
                "schema": { "type": "string" },
                "minOccurs": 0
            },
            "datetime": {
                "title": "Datetime",
                "description": "ISO 8601 datetime for the item",
                "schema": { "type": "string", "format": "date-time" },
                "minOccurs": 0
            },
            "properties": {
                "title": "Properties",
                "description": "Additional properties for the item",
                "schema": { "type": "object" },
                "minOccurs": 0
            },
            "skip_if_cog": {
                "title": "Skip if COG",
                "description": "Skip conversion if source is already a valid COG",
                "schema": { "type": "boolean", "default": true },
                "minOccurs": 0
            }
        },
        "outputs": {
            "item_id": {
                "title": "Item ID",
                "description": "ID of the created item",
                "schema": { "type": "string", "format": "uuid" }
            },
            "collection": {
                "title": "Collection",
                "description": "Collection the item was added to",
                "schema": { "type": "string" }
            },
            "asset_href": {
                "title": "Asset Href",
                "description": "S3 URI of the stored asset",
                "schema": { "type": "string", "format": "uri" }
            },
            "converted": {
                "title": "Converted",
                "description": "Whether the file was converted to COG",
                "schema": { "type": "boolean" }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_inline_input() {
        let inputs = ImportRasterInputs {
            collection: "test:collection".to_string(),
            data: InputValue::Inline(InlineValue {
                value: base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    b"test data",
                ),
                media_type: Some("image/tiff".to_string()),
            }),
            title: None,
            datetime: None,
            properties: None,
            skip_if_cog: true,
        };

        assert!(inputs.validate().is_ok());
    }

    #[test]
    fn test_validate_reference_input() {
        let inputs = ImportRasterInputs {
            collection: "test:collection".to_string(),
            data: InputValue::Reference(ReferenceValue {
                href: "s3://bucket/file.tif".to_string(),
                media_type: Some("image/tiff".to_string()),
            }),
            title: None,
            datetime: None,
            properties: None,
            skip_if_cog: true,
        };

        assert!(inputs.validate().is_ok());
    }

    #[test]
    fn test_validate_empty_collection() {
        let inputs = ImportRasterInputs {
            collection: "".to_string(),
            data: InputValue::Reference(ReferenceValue {
                href: "s3://bucket/file.tif".to_string(),
                media_type: None,
            }),
            title: None,
            datetime: None,
            properties: None,
            skip_if_cog: true,
        };

        assert!(inputs.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_href() {
        let inputs = ImportRasterInputs {
            collection: "test:collection".to_string(),
            data: InputValue::Reference(ReferenceValue {
                href: "ftp://invalid/url".to_string(),
                media_type: None,
            }),
            title: None,
            datetime: None,
            properties: None,
            skip_if_cog: true,
        };

        assert!(inputs.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_datetime() {
        let inputs = ImportRasterInputs {
            collection: "test:collection".to_string(),
            data: InputValue::Reference(ReferenceValue {
                href: "s3://bucket/file.tif".to_string(),
                media_type: None,
            }),
            title: None,
            datetime: Some("not-a-date".to_string()),
            properties: None,
            skip_if_cog: true,
        };

        assert!(inputs.validate().is_err());
    }
}
