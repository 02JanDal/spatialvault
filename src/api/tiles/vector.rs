/// Vector tile (MVT) generation utilities
use crate::error::{AppError, AppResult};

/// Common TileMatrixSet definitions
pub mod tile_matrix_sets {
    pub const WEB_MERCATOR_QUAD: &str = "WebMercatorQuad";
    pub const WORLD_CRS84_QUAD: &str = "WorldCRS84Quad";
}

/// Calculate tile bounds in Web Mercator
pub fn tile_bounds_web_mercator(z: u32, x: u32, y: u32) -> (f64, f64, f64, f64) {
    let n = 2_u32.pow(z) as f64;
    let tile_size = 40075016.685578488 / n; // Web Mercator extent / n

    let minx = -20037508.342789244 + (x as f64) * tile_size;
    let maxx = minx + tile_size;
    let maxy = 20037508.342789244 - (y as f64) * tile_size;
    let miny = maxy - tile_size;

    (minx, miny, maxx, maxy)
}

/// Calculate tile bounds in WGS84
pub fn tile_bounds_wgs84(z: u32, x: u32, y: u32) -> (f64, f64, f64, f64) {
    let n = 2_u32.pow(z) as f64;

    let lon_min = (x as f64) / n * 360.0 - 180.0;
    let lon_max = ((x + 1) as f64) / n * 360.0 - 180.0;

    let lat_max = (std::f64::consts::PI * (1.0 - 2.0 * (y as f64) / n))
        .sinh()
        .atan()
        .to_degrees();
    let lat_min = (std::f64::consts::PI * (1.0 - 2.0 * ((y + 1) as f64) / n))
        .sinh()
        .atan()
        .to_degrees();

    (lon_min, lat_min, lon_max, lat_max)
}

/// Generate ST_AsMVT SQL for a tile
pub fn mvt_sql(
    schema: &str,
    table: &str,
    geometry_column: &str,
    z: u32,
    x: u32,
    y: u32,
    storage_srid: i32,
) -> String {
    let (minx, miny, maxx, maxy) = tile_bounds_web_mercator(z, x, y);

    // Transform storage geometry to Web Mercator for tile generation
    let geom_transform = if storage_srid == 3857 {
        geometry_column.to_string()
    } else {
        format!("ST_Transform({}, 3857)", geometry_column)
    };

    format!(
        r#"
        WITH bounds AS (
            SELECT ST_MakeEnvelope({minx}, {miny}, {maxx}, {maxy}, 3857) AS geom
        ),
        mvtgeom AS (
            SELECT
                ST_AsMVTGeom(
                    {geom_transform},
                    bounds.geom,
                    4096,
                    256,
                    true
                ) AS geom,
                t.properties
            FROM "{schema}"."{table}" t, bounds
            WHERE ST_Intersects(
                {geom_transform},
                bounds.geom
            )
        )
        SELECT ST_AsMVT(mvtgeom.*, '{table}', 4096, 'geom') AS mvt
        FROM mvtgeom
        "#,
        minx = minx,
        miny = miny,
        maxx = maxx,
        maxy = maxy,
        geom_transform = geom_transform,
        schema = schema,
        table = table
    )
}

/// Validate tile coordinates
pub fn validate_tile_coords(z: u32, x: u32, y: u32, max_zoom: u32) -> AppResult<()> {
    if z > max_zoom {
        return Err(AppError::BadRequest(format!(
            "Zoom level {} exceeds maximum {}",
            z, max_zoom
        )));
    }

    let max_coord = 2_u32.pow(z);
    if x >= max_coord || y >= max_coord {
        return Err(AppError::NotFound(format!(
            "Tile {}/{}/{} out of bounds",
            z, x, y
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tile_bounds_wgs84() {
        let (minx, miny, maxx, maxy) = tile_bounds_wgs84(0, 0, 0);
        assert!((minx - (-180.0)).abs() < 0.001);
        assert!((maxx - 180.0).abs() < 0.001);
    }

    #[test]
    fn test_validate_tile_coords() {
        assert!(validate_tile_coords(0, 0, 0, 22).is_ok());
        assert!(validate_tile_coords(1, 1, 1, 22).is_ok());
        assert!(validate_tile_coords(1, 2, 0, 22).is_err()); // x out of bounds
        assert!(validate_tile_coords(25, 0, 0, 22).is_err()); // z too high
    }
}
