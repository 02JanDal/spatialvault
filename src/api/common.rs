use axum::extract::{FromRequest, Multipart, Request};
use axum::http::header::{CONTENT_TYPE, IF_MATCH};
use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::response::IntoResponse;
use axum_extra::TypedHeader;
use axum_extra::headers::{Error, Header, IfMatch};
use bytes::Bytes;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

    pub fn into_2d(self) -> [f64; 4] {
        match self {
            Bbox::TwoD(coords) => coords,
            Bbox::ThreeD(coords) => [coords[0], coords[1], coords[3], coords[4]],
        }
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

/// ETag utilities for versioning and optimistic locking
pub mod etag {
    use axum_extra::TypedHeader;
    use axum_extra::headers::{ETag, IfMatch};

    pub fn make(version: i64) -> ETag {
        format!("\"{}\"", version).parse().unwrap()
    }

    pub trait VersionMatch {
        fn matches(&self, version: i64) -> bool;
    }

    impl VersionMatch for Option<TypedHeader<IfMatch>> {
        fn matches(&self, version: i64) -> bool {
            self.as_ref().map_or(true, |if_match| {
                if_match.precondition_passes(&make(version))
            })
        }
    }
}

/// GeoJSON Geometry
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum GeoJsonGeometry {
    Point {
        coordinates: Vec<f64>,
    },
    MultiPoint {
        coordinates: Vec<Vec<f64>>,
    },
    LineString {
        coordinates: Vec<Vec<f64>>,
    },
    MultiLineString {
        coordinates: Vec<Vec<Vec<f64>>>,
    },
    Polygon {
        coordinates: Vec<Vec<Vec<f64>>>,
    },
    MultiPolygon {
        coordinates: Vec<Vec<Vec<Vec<f64>>>>,
    },
    GeometryCollection {
        geometries: Vec<GeoJsonGeometry>,
    },
}

/// STAC Asset
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Asset {
    pub href: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<String>>,
    #[serde(rename = "file:size", skip_serializing_if = "Option::is_none")]
    pub file_size: Option<i64>,
}

/// A map of asset keys to Asset objects
pub type Assets = HashMap<String, Asset>;

/// An uploaded file from a multipart request
#[derive(Debug)]
pub struct UploadedFile {
    pub filename: String,
    pub content_type: String,
    pub data: Bytes,
}

/// Parse a multipart request into a deserialized JSON field and a map of uploaded files.
///
/// One field named `json_field_name` is deserialized as `T`; all other fields are
/// collected as `UploadedFile` entries keyed by their field name.
pub async fn parse_multipart_with_files<S, T>(
    req: Request,
    state: &S,
    json_field_name: &str,
) -> Result<(T, HashMap<String, UploadedFile>), axum::response::Response>
where
    S: Send + Sync,
    T: serde::de::DeserializeOwned,
{
    let mut multipart = Multipart::from_request(req, state)
        .await
        .map_err(|e| e.into_response())?;

    let mut json_value: Option<T> = None;
    let mut files: HashMap<String, UploadedFile> = HashMap::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| e.into_response())?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == json_field_name {
            let bytes = field.bytes().await.map_err(|e| e.into_response())?;
            json_value = Some(serde_json::from_slice(&bytes).map_err(|e| {
                axum::response::Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(format!("Invalid {} JSON: {}", json_field_name, e).into())
                    .unwrap()
            })?);
        } else {
            let filename = field.file_name().unwrap_or(&name).to_string();
            let content_type = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();
            let data = field.bytes().await.map_err(|e| e.into_response())?;
            files.insert(
                name,
                UploadedFile {
                    filename,
                    content_type,
                    data,
                },
            );
        }
    }

    let json_value = json_value.ok_or_else(|| {
        axum::response::Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(format!("Missing '{}' field", json_field_name).into())
            .unwrap()
    })?;

    Ok((json_value, files))
}

/// Check if a request's Content-Type is multipart/form-data
pub fn is_multipart(req: &Request) -> bool {
    req.headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .starts_with("multipart/form-data")
}

/// Check if a request's Content-Type is JSON-like
/// (application/json, application/merge-patch+json, etc.)
pub fn is_json(req: &Request) -> bool {
    let ct = req
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json");
    ct.contains("json")
}

#[derive(Clone, Debug, PartialEq)]
pub struct Location(pub String);

impl Header for Location {
    fn name() -> &'static HeaderName {
        &header::LOCATION
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i HeaderValue>,
    {
        let value = values.next().ok_or_else(Error::invalid)?;
        value
            .to_str()
            .map_or(Err(Error::invalid()), |s| Ok(Location(s.to_string())))
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        values.extend(std::iter::once(HeaderValue::from_str(&self.0).unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::etag::VersionMatch;
    use axum_extra::TypedHeader;
    use axum_extra::headers::{Header, IfMatch};

    fn make_if_match(version: i64) -> Option<TypedHeader<IfMatch>> {
        let value = format!("\"{}\"", version).parse().unwrap();
        let if_match = IfMatch::decode(&mut std::iter::once(&value)).unwrap();
        Some(TypedHeader(if_match))
    }

    #[test]
    fn matches_returns_true_when_no_header() {
        let if_match: Option<TypedHeader<IfMatch>> = None;
        assert!(if_match.matches(1));
    }

    #[test]
    fn matches_returns_true_when_version_matches() {
        let if_match = make_if_match(42);
        assert!(if_match.matches(42));
    }

    #[test]
    fn matches_returns_false_when_version_differs() {
        let if_match = make_if_match(42);
        assert!(!if_match.matches(99));
    }
}

/**
 * axum's TypedHeader will return an empty string rather than a None if the header is missing, which
 * causes issues downstream. This function checks if the raw header is present and returns None if
 * not, allowing us to properly handle the case where the header is missing.
 */
pub fn fix_header(
    headers: axum::http::HeaderMap,
    if_match: Option<TypedHeader<IfMatch>>,
) -> Option<TypedHeader<IfMatch>> {
    if headers.contains_key(IF_MATCH) {
        if_match
    } else {
        None
    }
}
