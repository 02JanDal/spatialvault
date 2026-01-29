use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// OGC API Link object
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Link {
    pub href: String,
    pub rel: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hreflang: Option<String>,
}

impl Link {
    pub fn new(href: impl Into<String>, rel: impl Into<String>) -> Self {
        Self {
            href: href.into(),
            rel: rel.into(),
            media_type: None,
            title: None,
            hreflang: None,
        }
    }

    pub fn with_type(mut self, media_type: impl Into<String>) -> Self {
        self.media_type = Some(media_type.into());
        self
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

/// Standard link relations
pub mod rel {
    pub const SELF: &str = "self";
    pub const ALTERNATE: &str = "alternate";
    pub const CONFORMANCE: &str = "conformance";
    pub const DATA: &str = "data";
    pub const SERVICE_DESC: &str = "service-desc";
    pub const SERVICE_DOC: &str = "service-doc";
    pub const ITEMS: &str = "items";
    pub const ROOT: &str = "root";
    pub const PARENT: &str = "parent";
    pub const CHILD: &str = "child";
    pub const COLLECTION: &str = "collection";
    pub const NEXT: &str = "next";
    pub const PREV: &str = "prev";
    pub const FIRST: &str = "first";
    pub const LAST: &str = "last";
}

/// Standard media types
pub mod media_type {
    pub const JSON: &str = "application/json";
    pub const GEOJSON: &str = "application/geo+json";
    pub const OPENAPI_JSON: &str = "application/vnd.oai.openapi+json;version=3.0";
    pub const HTML: &str = "text/html";
    pub const MVT: &str = "application/vnd.mapbox-vector-tile";
    pub const PNG: &str = "image/png";
    pub const WEBP: &str = "image/webp";
    pub const TIFF: &str = "image/tiff";
    pub const COG: &str = "image/tiff; application=geotiff; profile=cloud-optimized";
    pub const COPC: &str = "application/vnd.laszip+copc";
}

/// Bounding box [minx, miny, maxx, maxy] or [minx, miny, minz, maxx, maxy, maxz]
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Bbox {
    TwoD([f64; 4]),
    ThreeD([f64; 6]),
}

impl Bbox {
    pub fn two_d(minx: f64, miny: f64, maxx: f64, maxy: f64) -> Self {
        Bbox::TwoD([minx, miny, maxx, maxy])
    }

    pub fn three_d(minx: f64, miny: f64, minz: f64, maxx: f64, maxy: f64, maxz: f64) -> Self {
        Bbox::ThreeD([minx, miny, minz, maxx, maxy, maxz])
    }
}

/// Temporal extent
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TemporalExtent {
    pub interval: Vec<[Option<String>; 2]>,
}

/// Spatial extent
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SpatialExtent {
    pub bbox: Vec<Bbox>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crs: Option<String>,
}

/// Combined extent (spatial + temporal)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Extent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spatial: Option<SpatialExtent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temporal: Option<TemporalExtent>,
}

/// CRS identifiers
pub mod crs {
    pub const WGS84: &str = "http://www.opengis.net/def/crs/OGC/1.3/CRS84";
    pub const WGS84_H: &str = "http://www.opengis.net/def/crs/OGC/0/CRS84h";
    pub const EPSG_4326: &str = "http://www.opengis.net/def/crs/EPSG/0/4326";
    pub const EPSG_3857: &str = "http://www.opengis.net/def/crs/EPSG/0/3857";

    /// Convert SRID to OGC CRS URI
    pub fn srid_to_uri(srid: i32) -> String {
        match srid {
            4326 => WGS84.to_string(),
            _ => format!("http://www.opengis.net/def/crs/EPSG/0/{}", srid),
        }
    }

    /// Convert OGC CRS URI to SRID
    pub fn uri_to_srid(uri: &str) -> Option<i32> {
        if uri == WGS84 || uri == WGS84_H {
            return Some(4326);
        }

        // Parse EPSG URIs
        if let Some(code) = uri.strip_prefix("http://www.opengis.net/def/crs/EPSG/0/") {
            return code.parse().ok();
        }

        None
    }
}

/// Query parameters for pagination
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct PaginationParams {
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
}

fn default_limit() -> u32 {
    10
}

impl PaginationParams {
    pub fn validate(&self) -> Result<(), String> {
        if self.limit > 10000 {
            return Err("Limit cannot exceed 10000".to_string());
        }
        Ok(())
    }
}
