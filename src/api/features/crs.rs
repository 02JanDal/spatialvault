use crate::api::common::crs::{srid_to_uri, uri_to_srid};
use crate::error::{AppResult, BadRequest};
use axum::http::{HeaderName, HeaderValue};
use axum_extra::headers::{Error, Header};
use snafu::OptionExt;

/// Parse CRS parameter and return SRID
pub fn parse_crs_param(crs: Option<&str>) -> AppResult<Option<i32>> {
    match crs {
        None => Ok(None),
        Some(uri) => {
            let srid = uri_to_srid(uri).context(BadRequest {
                message: format!("Unsupported CRS: {}", uri),
            })?;
            Ok(Some(srid))
        }
    }
}

pub struct ContentCrs(pub i32);
static CONTENTCRS: HeaderName = HeaderName::from_static("content-crs");
impl Header for ContentCrs {
    fn name() -> &'static HeaderName {
        &CONTENTCRS
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i HeaderValue>,
    {
        let value = values.next().ok_or_else(Error::invalid)?;
        let value = value.to_str().map_err(|_| Error::invalid())?;
        let value = value.trim_start_matches("<").trim_end_matches(">");

        uri_to_srid(value).map(ContentCrs).ok_or(Error::invalid())
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        values.extend(std::iter::once(
            HeaderValue::from_str(format!("<{}>", srid_to_uri(self.0)).as_str()).unwrap(),
        ));
    }
}

/// Build ST_Transform SQL fragment if needed
pub fn transform_geometry_sql(column: &str, source_srid: i32, target_srid: Option<i32>) -> String {
    match target_srid {
        Some(target) if target != source_srid => {
            format!("ST_Transform({}, {})", column, target)
        }
        _ => column.to_string(),
    }
}

/// Build bbox filter SQL with optional CRS transformation
pub fn bbox_filter_sql(
    geometry_column: &str,
    bbox: &[f64; 4],
    bbox_srid: i32,
    storage_srid: i32,
) -> String {
    let bbox_geom = format!(
        "ST_MakeEnvelope({}, {}, {}, {}, {})",
        bbox[0], bbox[1], bbox[2], bbox[3], bbox_srid
    );

    if bbox_srid != storage_srid {
        format!(
            "ST_Intersects({}, ST_Transform({}, {}))",
            geometry_column, bbox_geom, storage_srid
        )
    } else {
        format!("ST_Intersects({}, {})", geometry_column, bbox_geom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_crs_param() {
        assert_eq!(parse_crs_param(None).unwrap(), None);
        assert_eq!(
            parse_crs_param(Some("http://www.opengis.net/def/crs/OGC/1.3/CRS84")).unwrap(),
            Some(4326)
        );
        assert_eq!(
            parse_crs_param(Some("http://www.opengis.net/def/crs/EPSG/0/3857")).unwrap(),
            Some(3857)
        );
        assert!(parse_crs_param(Some("invalid")).is_err());
    }

    #[test]
    fn test_transform_geometry_sql() {
        assert_eq!(transform_geometry_sql("geom", 4326, None), "geom");
        assert_eq!(transform_geometry_sql("geom", 4326, Some(4326)), "geom");
        assert_eq!(
            transform_geometry_sql("geom", 4326, Some(3857)),
            "ST_Transform(geom, 3857)"
        );
    }

    #[test]
    fn test_bbox_filter_sql() {
        let bbox = [-180.0, -90.0, 180.0, 90.0];

        // Same SRID
        let sql = bbox_filter_sql("geom", &bbox, 4326, 4326);
        assert!(sql.contains("ST_Intersects"));
        assert!(!sql.contains("ST_Transform"));

        // Different SRID
        let sql = bbox_filter_sql("geom", &bbox, 4326, 3857);
        assert!(sql.contains("ST_Transform"));
    }
}
