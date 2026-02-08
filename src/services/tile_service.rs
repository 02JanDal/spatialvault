use std::sync::Arc;

use crate::api::tiles::raster::{RasterFormat, RasterTileParams, render_raster_tile};
use crate::api::tiles::vector::mvt_sql;
use crate::auth::quote_ident;
use crate::db::{Collection, Database};
use crate::error::{AppResult, BadRequest, Processing};

pub struct TileService {
    db: Arc<Database>,
}

impl TileService {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub async fn get_vector_tile(
        &self,
        username: &str,
        collection: &Collection,
        z: u32,
        x: u32,
        y: u32,
    ) -> AppResult<Vec<u8>> {
        if collection.collection_type != "vector" {
            return Err(BadRequest {
                message: "Vector tiles only available for vector collections".to_string(),
            }.build());
        }

        // Get storage SRID
        let storage_srid = self.get_storage_srid(&collection).await?;

        // Build MVT query
        let sql = mvt_sql(
            &collection.schema_name,
            &collection.table_name,
            "geometry",
            z,
            x,
            y,
            storage_srid,
        );

        let mut tx = self.db.begin_as(username).await?;
        let result: Option<(Vec<u8>,)> = sqlx::query_as(&sql).fetch_optional(&mut *tx).await?;

        Ok(result.map(|(data,)| data).unwrap_or_default())
    }

    pub async fn get_raster_tile(
        &self,
        _username: &str,
        collection: &Collection,
        z: u32,
        x: u32,
        y: u32,
        format: RasterFormat,
    ) -> AppResult<Vec<u8>> {
        if collection.collection_type != "raster" {
            return Err(BadRequest {
                message: "Raster tiles only available for raster collections".to_string(),
            }.build());
        }

        // Find COG files that intersect with the tile bounds
        let cog_hrefs = self.find_intersecting_cogs(&collection, z, x, y).await?;

        if cog_hrefs.is_empty() {
            // Return transparent/empty tile in requested format
            return create_empty_tile(256, format);
        }

        // For now, use the first intersecting COG
        // A full implementation would composite multiple COGs
        let cog_href = &cog_hrefs[0];

        let params = RasterTileParams {
            z,
            x,
            y,
            format,
            tile_size: 256,
        };

        // Run GDAL rendering in blocking task (GDAL is not async)
        let href = cog_href.clone();
        let result = tokio::task::spawn_blocking(move || render_raster_tile(&href, &params))
            .await
            .map_err(|e| Processing { message: format!("Task join error: {}", e) }.build())??;

        Ok(result)
    }

    /// Find COG files that intersect with the given tile
    async fn find_intersecting_cogs(
        &self,
        collection: &Collection,
        z: u32,
        x: u32,
        y: u32,
    ) -> AppResult<Vec<String>> {
        use crate::api::tiles::vector::tile_bounds_wgs84;

        let (minx, miny, maxx, maxy) = tile_bounds_wgs84(z, x, y);

        let assets: Vec<(String,)> = sqlx::query_as(&format!(
            r#"
            SELECT a.href
            FROM {0}.{1} a
            JOIN {0}.{2} i ON a.item_id = i.id
            WHERE a.key = 'data'
              AND ST_Intersects(i.geometry, ST_MakeEnvelope($2, $3, $4, $5, 4326))
            ORDER BY i.datetime DESC NULLS LAST
            LIMIT 10
            "#,
            quote_ident(&collection.schema_name),
            quote_ident(&format!("_{}_assets", collection.table_name)),
            quote_ident(&collection.table_name)
        ))
        .bind(collection.id)
        .bind(minx)
        .bind(miny)
        .bind(maxx)
        .bind(maxy)
        .fetch_all(self.db.pool())
        .await?;

        Ok(assets.into_iter().map(|(href,)| href).collect())
    }

    // TODO: move to CollectionService (same as with the same method in FeatureService)
    async fn get_storage_srid(&self, collection: &Collection) -> AppResult<i32> {
        let sql = r#"
            SELECT srid FROM geometry_columns
            WHERE f_table_schema = $1 AND f_table_name = $2
        "#;

        let result: Option<(i32,)> = sqlx::query_as(sql)
            .bind(&collection.schema_name)
            .bind(&collection.table_name)
            .fetch_optional(self.db.pool())
            .await?;

        Ok(result.map(|(srid,)| srid).unwrap_or(4326))
    }
}

/// Create an empty/transparent tile in the requested format
fn create_empty_tile(size: u32, format: RasterFormat) -> AppResult<Vec<u8>> {
    use crate::api::tiles::raster::{create_transparent_buffer, encode_image};

    let buffer = create_transparent_buffer(size as usize);
    encode_image(&buffer, size as usize, size as usize, format)
}
