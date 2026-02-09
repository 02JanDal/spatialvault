use std::path::Path;

use crate::error::{AppResult, BadRequest, Processing};
use gdal::vector::LayerAccess;

/// Vector file importer using GDAL
pub struct VectorImporter {
    layer: gdal::vector::Layer<'static>,
    _dataset: gdal::Dataset,
}

impl VectorImporter {
    /// Open a vector file (GeoJSON, Shapefile, GeoPackage, etc.)
    pub fn open(path: &Path) -> AppResult<Self> {
        let dataset = gdal::Dataset::open(path).map_err(|e| {
            Processing {
                message: format!("Failed to open vector file: {}", e),
            }
            .build()
        })?;

        // Get the first layer
        let layer = dataset.layer(0).map_err(|e| {
            Processing {
                message: format!("Failed to access layer: {}", e),
            }
            .build()
        })?;

        // SAFETY: We store the dataset alongside the layer to ensure it lives long enough
        // The layer borrows from the dataset, so we need to keep both alive
        // We transmute the lifetime to 'static since we control both lifetimes together
        let layer = unsafe { std::mem::transmute(layer) };

        Ok(Self {
            layer,
            _dataset: dataset,
        })
    }

    /// Get the source CRS as an EPSG code
    pub fn get_source_crs(&self) -> AppResult<i32> {
        let spatial_ref = self.layer.spatial_ref().ok_or_else(|| {
            Processing {
                message: "Layer has no spatial reference".to_string(),
            }
            .build()
        })?;

        // Try to get authority code (usually "EPSG")
        spatial_ref.auth_code().map_err(|_| {
            Processing {
                message: "Could not determine EPSG code from spatial reference".to_string(),
            }
            .build()
        })
    }

    /// Get the total feature count
    pub fn feature_count(&self) -> AppResult<i64> {
        Ok(self.layer.feature_count() as i64)
    }

    /// Read features in batches
    pub fn read_features_batch(&mut self, batch_size: usize) -> AppResult<Vec<VectorFeature>> {
        let mut features = Vec::with_capacity(batch_size);

        for feature in self.layer.features().take(batch_size) {
            // Extract geometry as WKT
            let geometry = feature.geometry().ok_or_else(|| {
                Processing {
                    message: "Feature has no geometry".to_string(),
                }
                .build()
            })?;

            let geometry_wkt = geometry.wkt().map_err(|e| {
                Processing {
                    message: format!("Failed to convert geometry to WKT: {}", e),
                }
                .build()
            })?;

            // Extract properties as JSON
            let properties = extract_properties(&feature)?;

            features.push(VectorFeature {
                geometry_wkt,
                properties,
            });
        }

        Ok(features)
    }

    /// Get a list of supported vector formats
    pub fn supported_formats() -> Vec<&'static str> {
        vec![
            "GeoJSON",
            "ESRI Shapefile",
            "GeoPackage",
            "GML",
            "KML",
            "FlatGeobuf",
        ]
    }
}

/// A feature extracted from a vector file
#[derive(Debug)]
pub struct VectorFeature {
    pub geometry_wkt: String,
    pub properties: serde_json::Value,
}

/// Extract feature properties as a JSON object
fn extract_properties(feature: &gdal::vector::Feature) -> AppResult<serde_json::Value> {
    let mut properties = serde_json::Map::new();

    // Iterate through all fields in the feature
    for (field_name, field_value) in feature.fields() {
        let value = match field_value {
            Some(gdal::vector::FieldValue::IntegerValue(i)) => {
                serde_json::Value::Number(i.into())
            }
            Some(gdal::vector::FieldValue::Integer64Value(i)) => {
                serde_json::Value::Number(i.into())
            }
            Some(gdal::vector::FieldValue::RealValue(f)) => {
                serde_json::Value::Number(
                    serde_json::Number::from_f64(f)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                )
            }
            Some(gdal::vector::FieldValue::StringValue(s)) => {
                serde_json::Value::String(s.clone())
            }
            Some(gdal::vector::FieldValue::DateValue(date)) => {
                serde_json::Value::String(date.to_string())
            }
            Some(gdal::vector::FieldValue::DateTimeValue(dt)) => {
                serde_json::Value::String(dt.to_rfc3339())
            }
            None => serde_json::Value::Null,
            _ => serde_json::Value::Null, // For other field types we don't handle
        };

        properties.insert(field_name.to_string(), value);
    }

    Ok(serde_json::Value::Object(properties))
}

/// Detect file extension from file content or path
pub fn detect_extension(path: &Path) -> &str {
    match path.extension().and_then(|s| s.to_str()) {
        Some("geojson") | Some("json") => "geojson",
        Some("shp") => "shp",
        Some("gpkg") => "gpkg",
        Some("gml") => "gml",
        Some("kml") => "kml",
        Some("fgb") => "fgb",
        _ => "unknown",
    }
}

/// Validate that a file is a supported vector format
pub fn validate_vector_format(path: &Path) -> AppResult<()> {
    // Try to open the file with GDAL
    let dataset = gdal::Dataset::open(path).map_err(|e| {
        BadRequest {
            message: format!(
                "Unsupported or invalid file format. Supported formats: {}. Error: {}",
                VectorImporter::supported_formats().join(", "),
                e
            ),
        }
        .build()
    })?;

    // Verify it has at least one layer
    if dataset.layer_count() == 0 {
        return BadRequest {
            message: "File contains no layers".to_string(),
        }
        .fail();
    }

    Ok(())
}
