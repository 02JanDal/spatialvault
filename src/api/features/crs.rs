use crate::api::common::crs::{srid_to_uri, uri_to_srid};
use crate::error::{AppError, AppResult};

/// Parse CRS parameter and return SRID
pub fn parse_crs_param(crs: Option<&str>) -> AppResult<Option<i32>> {
    match crs {
        None => Ok(None),
        Some(uri) => {
            let srid = uri_to_srid(uri)
                .ok_or_else(|| AppError::BadRequest(format!("Unsupported CRS: {}", uri)))?;
            Ok(Some(srid))
        }
    }
}

/// Generate Content-Crs header value
pub fn content_crs_header(srid: i32) -> String {
    format!("<{}>", srid_to_uri(srid))
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
    fn test_content_crs_header() {
        assert_eq!(
            content_crs_header(4326),
            "<http://www.opengis.net/def/crs/OGC/1.3/CRS84>"
        );
        assert_eq!(
            content_crs_header(3857),
            "<http://www.opengis.net/def/crs/EPSG/0/3857>"
        );
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
