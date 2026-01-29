use std::sync::Arc;

use crate::api::coverages::handlers::{
    DomainSet, GeneralGrid, GridAxis, RangeField, RangeType, UnitOfMeasure,
};
use crate::api::coverages::range_subset::CoverageSubsetParams;
use crate::db::{Collection, Database};
use crate::error::{AppError, AppResult};

pub struct CoverageService {
    db: Arc<Database>,
}

/// Collection extent derived from items
struct CollectionExtent {
    minx: f64,
    miny: f64,
    maxx: f64,
    maxy: f64,
}

impl CoverageService {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub async fn get_collection(
        &self,
        _username: &str,
        collection_id: &str,
    ) -> AppResult<Option<Collection>> {
        let collection: Option<Collection> =
            sqlx::query_as("SELECT * FROM spatialvault.collections WHERE canonical_name = $1")
                .bind(collection_id)
                .fetch_optional(self.db.pool())
                .await?;

        Ok(collection)
    }

    /// Get the spatial extent of a collection by aggregating item geometries
    async fn get_collection_extent(&self, collection_id: uuid::Uuid) -> AppResult<CollectionExtent> {
        let sql = r#"
            SELECT
                ST_XMin(ST_Extent(geometry)) as minx,
                ST_YMin(ST_Extent(geometry)) as miny,
                ST_XMax(ST_Extent(geometry)) as maxx,
                ST_YMax(ST_Extent(geometry)) as maxy
            FROM spatialvault.items
            WHERE collection_id = $1
        "#;

        let extent: Option<(Option<f64>, Option<f64>, Option<f64>, Option<f64>)> =
            sqlx::query_as(sql)
                .bind(collection_id)
                .fetch_optional(self.db.pool())
                .await?;

        match extent {
            Some((Some(minx), Some(miny), Some(maxx), Some(maxy))) => {
                Ok(CollectionExtent { minx, miny, maxx, maxy })
            }
            _ => {
                // Default to global extent if no items
                Ok(CollectionExtent {
                    minx: -180.0,
                    miny: -90.0,
                    maxx: 180.0,
                    maxy: 90.0,
                })
            }
        }
    }

    pub async fn get_domainset(
        &self,
        username: &str,
        collection_id: &str,
    ) -> AppResult<DomainSet> {
        let collection = self
            .get_collection(username, collection_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Collection not found: {}", collection_id)))?;

        if collection.collection_type != "raster" {
            return Err(AppError::BadRequest(
                "Coverages only available for raster collections".to_string(),
            ));
        }

        // Get actual extent from items
        let extent = self.get_collection_extent(collection.id).await?;

        // Estimate resolution based on extent (this would ideally come from COG metadata)
        let x_range = extent.maxx - extent.minx;
        let y_range = extent.maxy - extent.miny;
        let estimated_resolution = f64::min(x_range, y_range) / 1000.0; // Rough estimate

        Ok(DomainSet {
            domain_type: "DomainSet".to_string(),
            general_grid: GeneralGrid {
                grid_type: "GeneralGridCoverage".to_string(),
                srs_name: "http://www.opengis.net/def/crs/EPSG/0/4326".to_string(),
                axis_labels: vec!["Lat".to_string(), "Long".to_string()],
                axis: vec![
                    GridAxis {
                        axis_type: "RegularAxis".to_string(),
                        axis_label: "Lat".to_string(),
                        lower_bound: extent.miny,
                        upper_bound: extent.maxy,
                        resolution: estimated_resolution.max(0.0001),
                        uom_label: "deg".to_string(),
                    },
                    GridAxis {
                        axis_type: "RegularAxis".to_string(),
                        axis_label: "Long".to_string(),
                        lower_bound: extent.minx,
                        upper_bound: extent.maxx,
                        resolution: estimated_resolution.max(0.0001),
                        uom_label: "deg".to_string(),
                    },
                ],
            },
        })
    }

    pub async fn get_rangetype(
        &self,
        username: &str,
        collection_id: &str,
    ) -> AppResult<RangeType> {
        let collection = self
            .get_collection(username, collection_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Collection not found: {}", collection_id)))?;

        if collection.collection_type != "raster" {
            return Err(AppError::BadRequest(
                "Coverages only available for raster collections".to_string(),
            ));
        }

        // Get number of items to use as hint for band count
        // In a full implementation, we would read the COG metadata
        let item_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM spatialvault.items WHERE collection_id = $1",
        )
        .bind(collection.id)
        .fetch_one(self.db.pool())
        .await?;

        // Default to single band description (would be enhanced with GDAL metadata)
        Ok(RangeType {
            range_type: "DataRecord".to_string(),
            field: vec![RangeField {
                field_type: "Quantity".to_string(),
                id: "band1".to_string(),
                name: "Band 1".to_string(),
                description: Some(format!(
                    "Raster data from {} items",
                    item_count.0
                )),
                definition: "http://www.opengis.net/def/property/OGC/0/Radiance".to_string(),
                uom: UnitOfMeasure {
                    uom_type: "UnitReference".to_string(),
                    code: "1".to_string(), // Dimensionless by default
                },
            }],
        })
    }

    pub async fn get_coverage_data(
        &self,
        username: &str,
        collection_id: &str,
        _params: &CoverageSubsetParams,
    ) -> AppResult<Vec<u8>> {
        let collection = self
            .get_collection(username, collection_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Collection not found: {}", collection_id)))?;

        if collection.collection_type != "raster" {
            return Err(AppError::BadRequest(
                "Coverages only available for raster collections".to_string(),
            ));
        }

        // Get the primary asset for the first item in the collection
        // In a full implementation, we would:
        // 1. Select items based on subsetting parameters
        // 2. Use GDAL to read and transform the COG data
        // 3. Apply any requested subsetting/resampling

        let asset: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT a.href
            FROM spatialvault.assets a
            JOIN spatialvault.items i ON a.item_id = i.id
            WHERE i.collection_id = $1 AND a.key = 'data'
            LIMIT 1
            "#,
        )
        .bind(collection.id)
        .fetch_optional(self.db.pool())
        .await?;

        match asset {
            Some((href,)) => {
                // For now, return a redirect hint in the error
                // A full implementation would use GDAL to read the COG
                Err(AppError::Internal(format!(
                    "Coverage data available at: {}. Direct access requires GDAL integration.",
                    href
                )))
            }
            None => Err(AppError::NotFound(
                "No raster data available for this collection".to_string(),
            )),
        }
    }

    /// Get asset URLs for a collection (useful for clients that can read COGs directly)
    pub async fn get_collection_assets(
        &self,
        collection_id: &str,
    ) -> AppResult<Vec<(String, String)>> {
        let collection: Option<Collection> =
            sqlx::query_as("SELECT * FROM spatialvault.collections WHERE canonical_name = $1")
                .bind(collection_id)
                .fetch_optional(self.db.pool())
                .await?;

        let collection = collection
            .ok_or_else(|| AppError::NotFound(format!("Collection not found: {}", collection_id)))?;

        let assets: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT i.id::text, a.href
            FROM spatialvault.assets a
            JOIN spatialvault.items i ON a.item_id = i.id
            WHERE i.collection_id = $1 AND a.key = 'data'
            ORDER BY i.datetime DESC NULLS LAST
            "#,
        )
        .bind(collection.id)
        .fetch_all(self.db.pool())
        .await?;

        Ok(assets)
    }
}
