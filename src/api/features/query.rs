use schemars::JsonSchema;
use serde::Deserialize;

use crate::error::{AppError, AppResult};

// Re-export cql2 crate for parsing
pub use cql2;

/// Query parameters for listing features
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct FeatureQueryParams {
    /// Maximum number of features to return
    #[serde(default = "default_limit")]
    pub limit: u32,

    /// Offset for pagination
    #[serde(default)]
    pub offset: u32,

    /// Bounding box filter: minx,miny,maxx,maxy
    pub bbox: Option<String>,

    /// CRS for bbox coordinates
    pub bbox_crs: Option<String>,

    /// CRS for response geometry
    pub crs: Option<String>,

    /// Temporal filter: datetime or interval
    pub datetime: Option<String>,

    /// CQL2 filter expression (text format)
    pub filter: Option<String>,

    /// Filter language: cql2-text or cql2-json
    pub filter_lang: Option<String>,

    /// CRS for filter geometry values
    pub filter_crs: Option<String>,

    /// Property names to include in response
    pub properties: Option<String>,

    /// Sort by property (prefix with - for descending)
    pub sortby: Option<String>,
}

fn default_limit() -> u32 {
    10
}

impl FeatureQueryParams {
    pub fn validate(&self) -> AppResult<()> {
        if self.limit == 0 {
            return Err(AppError::BadRequest(
                "Limit must be at least 1".to_string(),
            ));
        }

        if self.limit > 10000 {
            return Err(AppError::BadRequest(
                "Limit cannot exceed 10000".to_string(),
            ));
        }

        if let Some(ref bbox) = self.bbox {
            let coords = self.parse_bbox(bbox)?;
            // Validate bbox coordinates are sensible
            if coords[0] >= coords[2] {
                return Err(AppError::BadRequest(
                    "bbox minx must be less than maxx".to_string(),
                ));
            }
            if coords[1] >= coords[3] {
                return Err(AppError::BadRequest(
                    "bbox miny must be less than maxy".to_string(),
                ));
            }
        }

        if let Some(ref dt) = self.datetime {
            self.validate_datetime(dt)?;
        }

        Ok(())
    }

    /// Parse bbox string into array of coordinates
    pub fn parse_bbox(&self, bbox: &str) -> AppResult<[f64; 4]> {
        let parts: Vec<&str> = bbox.split(',').collect();
        if parts.len() != 4 {
            return Err(AppError::BadRequest(
                "bbox must have 4 values: minx,miny,maxx,maxy".to_string(),
            ));
        }

        let mut coords = [0.0f64; 4];
        for (i, part) in parts.iter().enumerate() {
            coords[i] = part
                .trim()
                .parse::<f64>()
                .map_err(|_| AppError::BadRequest(format!("Invalid bbox coordinate '{}': must be a number", part.trim())))?;

            // Check for NaN and Infinity
            if !coords[i].is_finite() {
                return Err(AppError::BadRequest(format!(
                    "Invalid bbox coordinate '{}': must be a finite number",
                    part.trim()
                )));
            }
        }

        Ok(coords)
    }

    /// Validate datetime parameter
    fn validate_datetime(&self, datetime: &str) -> AppResult<()> {
        // Datetime can be:
        // - Single instant: 2023-01-01T00:00:00Z
        // - Open interval: ../2023-01-01T00:00:00Z or 2023-01-01T00:00:00Z/..
        // - Closed interval: 2023-01-01T00:00:00Z/2023-12-31T23:59:59Z

        if datetime.contains('/') {
            let parts: Vec<&str> = datetime.split('/').collect();
            if parts.len() != 2 {
                return Err(AppError::BadRequest(
                    "Invalid datetime interval format".to_string(),
                ));
            }
            // ".." represents open-ended
            for part in parts {
                if part != ".." {
                    self.validate_datetime_instant(part)?;
                }
            }
        } else {
            self.validate_datetime_instant(datetime)?;
        }

        Ok(())
    }

    fn validate_datetime_instant(&self, instant: &str) -> AppResult<()> {
        // Basic ISO 8601 validation
        chrono::DateTime::parse_from_rfc3339(instant)
            .map_err(|_| AppError::BadRequest(format!("Invalid datetime: {}", instant)))?;
        Ok(())
    }

    /// Parse properties parameter into list of property names
    pub fn parse_properties(&self) -> Option<Vec<String>> {
        self.properties.as_ref().map(|p| {
            p.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
    }

    /// Parse sortby parameter
    pub fn parse_sortby(&self) -> Option<Vec<(String, bool)>> {
        self.sortby.as_ref().map(|s| {
            s.split(',')
                .map(|part| {
                    let part = part.trim();
                    if let Some(prop) = part.strip_prefix('-') {
                        (prop.to_string(), false) // descending
                    } else if let Some(prop) = part.strip_prefix('+') {
                        (prop.to_string(), true) // ascending
                    } else {
                        (part.to_string(), true) // default ascending
                    }
                })
                .collect()
        })
    }
}

/// CQL2 parser using the cql2 crate with PostGIS-compatible SQL output
pub struct Cql2Parser;

impl Cql2Parser {
    /// Parse a CQL2-text filter into SQL WHERE clause using the cql2 crate
    pub fn parse_to_sql(filter: &str, property_prefix: &str) -> AppResult<String> {
        let filter = filter.trim();

        // Parse using the cql2 crate
        let expr = cql2::parse_text(filter)
            .map_err(|e| AppError::BadRequest(format!("CQL2 parse error: {}", e)))?;

        // Convert to PostGIS-compatible SQL
        Self::expr_to_postgis_sql(&expr, property_prefix)
    }

    /// Parse a CQL2-json filter into SQL WHERE clause
    pub fn parse_json_to_sql(filter: &str, property_prefix: &str) -> AppResult<String> {
        let expr = cql2::parse_json(filter)
            .map_err(|e| AppError::BadRequest(format!("CQL2 JSON parse error: {}", e)))?;

        Self::expr_to_postgis_sql(&expr, property_prefix)
    }

    /// Convert a CQL2 expression to PostGIS-compatible SQL
    fn expr_to_postgis_sql(expr: &cql2::Expr, prefix: &str) -> AppResult<String> {
        match expr {
            // Boolean literals
            cql2::Expr::Bool(b) => Ok(if *b { "TRUE" } else { "FALSE" }.to_string()),

            // Numeric literals
            cql2::Expr::Float(f) => Ok(f.to_string()),

            // String literals
            cql2::Expr::Literal(s) => Ok(format!("'{}'", s.replace('\'', "''"))),

            // Property reference
            cql2::Expr::Property { property } => Ok(Self::property_to_sql(property, prefix)),

            // Null
            cql2::Expr::Null => Ok("NULL".to_string()),

            // Date (contains a boxed Expr that should be a Literal)
            cql2::Expr::Date { date } => {
                let date_str = Self::expr_to_postgis_sql(date, prefix)?;
                Ok(format!("DATE {}", date_str))
            }

            // Timestamp (contains a boxed Expr that should be a Literal)
            cql2::Expr::Timestamp { timestamp } => {
                let ts_str = Self::expr_to_postgis_sql(timestamp, prefix)?;
                Ok(format!("TIMESTAMP {}", ts_str))
            }

            // Interval (contains a vec of expressions)
            cql2::Expr::Interval { interval } => {
                if interval.len() != 2 {
                    return Err(AppError::BadRequest("Interval must have 2 elements".to_string()));
                }
                let start_sql = Self::expr_to_postgis_sql(&interval[0], prefix)?;
                let end_sql = Self::expr_to_postgis_sql(&interval[1], prefix)?;
                Ok(format!("TSTZRANGE({}, {})", start_sql, end_sql))
            }

            // BBox
            cql2::Expr::BBox { bbox } => {
                if bbox.len() < 4 {
                    return Err(AppError::BadRequest("BBox must have at least 4 elements".to_string()));
                }
                let coords: Vec<String> = bbox
                    .iter()
                    .map(|e| Self::expr_to_postgis_sql(e, prefix))
                    .collect::<AppResult<Vec<_>>>()?;
                Ok(format!(
                    "ST_MakeEnvelope({}, {}, {}, {}, 4326)",
                    coords[0], coords[1], coords[2], coords[3]
                ))
            }

            // Geometry
            cql2::Expr::Geometry(geom) => Self::geometry_to_sql(geom),

            // Array
            cql2::Expr::Array(items) => {
                let items_sql: Vec<String> = items
                    .iter()
                    .map(|e| Self::expr_to_postgis_sql(e, prefix))
                    .collect::<AppResult<Vec<_>>>()?;
                Ok(format!("ARRAY[{}]", items_sql.join(", ")))
            }

            // All operations (AND, OR, =, >, spatial functions, etc.)
            cql2::Expr::Operation { op, args } => {
                Self::operation_to_sql(op, args, prefix)
            }
        }
    }

    /// Convert CQL2 operations to PostGIS SQL
    fn operation_to_sql(op: &str, args: &[Box<cql2::Expr>], prefix: &str) -> AppResult<String> {
        let op_lower = op.to_lowercase();

        // Binary comparison/logical operators
        match op_lower.as_str() {
            "and" => {
                let parts: Vec<String> = args
                    .iter()
                    .map(|a| Self::expr_to_postgis_sql(a, prefix))
                    .collect::<AppResult<Vec<_>>>()?;
                return Ok(format!("({})", parts.join(" AND ")));
            }
            "or" => {
                let parts: Vec<String> = args
                    .iter()
                    .map(|a| Self::expr_to_postgis_sql(a, prefix))
                    .collect::<AppResult<Vec<_>>>()?;
                return Ok(format!("({})", parts.join(" OR ")));
            }
            "not" => {
                if args.len() != 1 {
                    return Err(AppError::BadRequest("NOT requires 1 argument".to_string()));
                }
                let inner = Self::expr_to_postgis_sql(&args[0], prefix)?;
                return Ok(format!("NOT ({})", inner));
            }
            "=" | "eq" => return Self::binary_op(args, "=", prefix),
            "<>" | "!=" | "neq" => return Self::binary_op(args, "<>", prefix),
            "<" | "lt" => return Self::binary_op(args, "<", prefix),
            ">" | "gt" => return Self::binary_op(args, ">", prefix),
            "<=" | "lte" => return Self::binary_op(args, "<=", prefix),
            ">=" | "gte" => return Self::binary_op(args, ">=", prefix),
            "+" => return Self::binary_op(args, "+", prefix),
            "-" => return Self::binary_op(args, "-", prefix),
            "*" => return Self::binary_op(args, "*", prefix),
            "/" => return Self::binary_op(args, "/", prefix),
            "%" => return Self::binary_op(args, "%", prefix),
            "like" => return Self::binary_op(args, "LIKE", prefix),
            "ilike" => return Self::binary_op(args, "ILIKE", prefix),
            "between" => {
                if args.len() != 3 {
                    return Err(AppError::BadRequest("BETWEEN requires 3 arguments".to_string()));
                }
                let val = Self::expr_to_postgis_sql(&args[0], prefix)?;
                let lower = Self::expr_to_postgis_sql(&args[1], prefix)?;
                let upper = Self::expr_to_postgis_sql(&args[2], prefix)?;
                return Ok(format!("{} BETWEEN {} AND {}", val, lower, upper));
            }
            "in" => {
                if args.len() < 2 {
                    return Err(AppError::BadRequest("IN requires at least 2 arguments".to_string()));
                }
                let val = Self::expr_to_postgis_sql(&args[0], prefix)?;
                let list: Vec<String> = args[1..]
                    .iter()
                    .map(|a| Self::expr_to_postgis_sql(a, prefix))
                    .collect::<AppResult<Vec<_>>>()?;
                return Ok(format!("{} IN ({})", val, list.join(", ")));
            }
            "isnull" | "is null" => {
                if args.len() != 1 {
                    return Err(AppError::BadRequest("IS NULL requires 1 argument".to_string()));
                }
                let inner = Self::expr_to_postgis_sql(&args[0], prefix)?;
                return Ok(format!("{} IS NULL", inner));
            }
            _ => {}
        }

        // Spatial functions - map CQL2 names to PostGIS
        let spatial_mapping: &[(&str, &str)] = &[
            ("s_intersects", "ST_Intersects"),
            ("s_contains", "ST_Contains"),
            ("s_within", "ST_Within"),
            ("s_crosses", "ST_Crosses"),
            ("s_overlaps", "ST_Overlaps"),
            ("s_touches", "ST_Touches"),
            ("s_disjoint", "ST_Disjoint"),
            ("s_equals", "ST_Equals"),
        ];

        for (cql_name, pg_name) in spatial_mapping {
            if op_lower == *cql_name {
                if args.len() != 2 {
                    return Err(AppError::BadRequest(format!(
                        "{} requires 2 arguments",
                        op
                    )));
                }
                let arg1 = Self::expr_to_postgis_sql(&args[0], prefix)?;
                let arg2 = Self::expr_to_postgis_sql(&args[1], prefix)?;
                return Ok(format!("{}({}, {})", pg_name, arg1, arg2));
            }
        }

        // S_DWITHIN (distance within)
        if op_lower == "s_dwithin" {
            if args.len() != 3 {
                return Err(AppError::BadRequest(
                    "S_DWITHIN requires 3 arguments".to_string(),
                ));
            }
            let geom1 = Self::expr_to_postgis_sql(&args[0], prefix)?;
            let geom2 = Self::expr_to_postgis_sql(&args[1], prefix)?;
            let distance = Self::expr_to_postgis_sql(&args[2], prefix)?;
            return Ok(format!("ST_DWithin({}, {}, {})", geom1, geom2, distance));
        }

        // Temporal functions
        if op_lower == "t_intersects" {
            if args.len() != 2 {
                return Err(AppError::BadRequest(
                    "T_INTERSECTS requires 2 arguments".to_string(),
                ));
            }
            let time1 = Self::expr_to_postgis_sql(&args[0], prefix)?;
            let time2 = Self::expr_to_postgis_sql(&args[1], prefix)?;
            return Ok(format!("({} && {})", time1, time2));
        }

        // Array functions
        if op_lower == "a_contains" {
            if args.len() != 2 {
                return Err(AppError::BadRequest(
                    "A_CONTAINS requires 2 arguments".to_string(),
                ));
            }
            let arr = Self::expr_to_postgis_sql(&args[0], prefix)?;
            let val = Self::expr_to_postgis_sql(&args[1], prefix)?;
            return Ok(format!("({} @> {})", arr, val));
        }

        // Generic function call
        let args_sql: Vec<String> = args
            .iter()
            .map(|a| Self::expr_to_postgis_sql(a, prefix))
            .collect::<AppResult<Vec<_>>>()?;

        Ok(format!("{}({})", op.to_uppercase(), args_sql.join(", ")))
    }

    /// Helper for binary operators
    fn binary_op(args: &[Box<cql2::Expr>], sql_op: &str, prefix: &str) -> AppResult<String> {
        if args.len() != 2 {
            return Err(AppError::BadRequest(format!(
                "{} requires 2 arguments",
                sql_op
            )));
        }
        let left = Self::expr_to_postgis_sql(&args[0], prefix)?;
        let right = Self::expr_to_postgis_sql(&args[1], prefix)?;
        Ok(format!("({} {} {})", left, sql_op, right))
    }

    /// Convert CQL2 geometry to PostGIS
    fn geometry_to_sql(geom: &cql2::Geometry) -> AppResult<String> {
        match geom {
            cql2::Geometry::Wkt(wkt) => {
                Ok(format!("ST_GeomFromText('{}', 4326)", wkt.replace('\'', "''")))
            }
            cql2::Geometry::GeoJSON(geojson) => {
                let json_str = serde_json::to_string(geojson)
                    .map_err(|e| AppError::Internal(format!("Failed to serialize GeoJSON: {}", e)))?;
                Ok(format!("ST_GeomFromGeoJSON('{}')", json_str.replace('\'', "''")))
            }
        }
    }

    /// Convert property name to SQL
    fn property_to_sql(property: &str, prefix: &str) -> String {
        if property.contains('.') {
            // Nested property access via JSONB
            let parts: Vec<&str> = property.split('.').collect();
            if parts[0] == "properties" {
                // Access into properties JSONB column
                format!(
                    "{}properties->>'{}'",
                    prefix,
                    parts[1..].join("'->>")
                )
            } else {
                format!("{}\"{}\"", prefix, parts[0])
            }
        } else if property == "geometry" {
            format!("{}geometry", prefix)
        } else {
            // Column reference
            format!("{}\"{}\"", prefix, property)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bbox() {
        let params = FeatureQueryParams::default();
        let bbox = params.parse_bbox("-180,-90,180,90").unwrap();
        assert_eq!(bbox, [-180.0, -90.0, 180.0, 90.0]);

        assert!(params.parse_bbox("invalid").is_err());
        assert!(params.parse_bbox("1,2,3").is_err());
        assert!(params.parse_bbox("NaN,0,1,1").is_err());
        assert!(params.parse_bbox("inf,0,1,1").is_err());
    }

    #[test]
    fn test_validate_limit() {
        let mut params = FeatureQueryParams::default();

        // Zero limit should fail
        params.limit = 0;
        assert!(params.validate().is_err());

        // Valid limit
        params.limit = 10;
        params.bbox = None;
        params.datetime = None;
        assert!(params.validate().is_ok());

        // Excessive limit should fail
        params.limit = 10001;
        assert!(params.validate().is_err());
    }

    #[test]
    fn test_validate_bbox_bounds() {
        let mut params = FeatureQueryParams::default();
        params.limit = 10;
        params.datetime = None;

        // Valid bbox
        params.bbox = Some("-180,-90,180,90".to_string());
        assert!(params.validate().is_ok());

        // Invalid: minx >= maxx
        params.bbox = Some("10,0,5,10".to_string());
        assert!(params.validate().is_err());

        // Invalid: miny >= maxy
        params.bbox = Some("0,10,10,5".to_string());
        assert!(params.validate().is_err());
    }

    #[test]
    fn test_parse_properties() {
        let mut params = FeatureQueryParams::default();
        params.properties = Some("name,population,area".to_string());

        let props = params.parse_properties().unwrap();
        assert_eq!(props, vec!["name", "population", "area"]);
    }

    #[test]
    fn test_parse_sortby() {
        let mut params = FeatureQueryParams::default();
        params.sortby = Some("-population,+name,area".to_string());

        let sort = params.parse_sortby().unwrap();
        assert_eq!(sort[0], ("population".to_string(), false));
        assert_eq!(sort[1], ("name".to_string(), true));
        assert_eq!(sort[2], ("area".to_string(), true));
    }

    #[test]
    fn test_cql2_comparison() {
        // Test equality
        let sql = Cql2Parser::parse_to_sql("name = 'Berlin'", "").unwrap();
        assert!(sql.contains("="));
        assert!(sql.contains("Berlin"));

        // Test greater than
        let sql = Cql2Parser::parse_to_sql("population > 1000000", "").unwrap();
        assert!(sql.contains(">"));
        assert!(sql.contains("1000000"));
    }

    #[test]
    fn test_cql2_logical() {
        let sql = Cql2Parser::parse_to_sql("name = 'Berlin' AND population > 1000000", "").unwrap();
        assert!(sql.to_uppercase().contains("AND"));

        let sql = Cql2Parser::parse_to_sql("type = 'city' OR type = 'town'", "").unwrap();
        assert!(sql.to_uppercase().contains("OR"));
    }

    #[test]
    fn test_cql2_spatial_intersects() {
        // Test with WKT geometry
        let sql = Cql2Parser::parse_to_sql(
            "S_INTERSECTS(geometry, POLYGON((0 0, 1 0, 1 1, 0 1, 0 0)))",
            "",
        )
        .unwrap();
        assert!(sql.contains("ST_Intersects"));
        assert!(sql.contains("POLYGON"));
    }

    #[test]
    fn test_cql2_property_prefix() {
        let sql = Cql2Parser::parse_to_sql("name = 'test'", "t.").unwrap();
        assert!(sql.contains("t."));
    }
}
