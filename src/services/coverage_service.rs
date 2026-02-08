use crate::api::Bbox;
use crate::api::coverages::handlers::{
    DomainSet, GeneralGrid, GridAxis, RangeField, RangeType, UnitOfMeasure,
};
use crate::api::coverages::range_subset::CoverageSubsetParams;
use crate::db::{Collection, Database};
use crate::error::{AppResult, BadRequest, Internal, NotFound};
use crate::services::CollectionService;
use std::sync::Arc;

pub struct CoverageService {
    db: Arc<Database>,
    collection_service: Arc<CollectionService>,
}

impl CoverageService {
    pub fn new(db: Arc<Database>, collection_service: Arc<CollectionService>) -> Self {
        Self {
            db,
            collection_service,
        }
    }

    pub async fn get_domainset(
        &self,
        username: &str,
        collection: &Collection,
    ) -> AppResult<DomainSet> {
        if collection.collection_type != "raster" {
            return Err(BadRequest {
                message: "Coverages only available for raster collections".to_string(),
            }.build());
        }

        // Get actual extent from items
        let [minx, miny, maxx, maxy] = self
            .collection_service
            .get_collection_extent(collection, 4326)
            .await?
            .unwrap_or(Bbox::two_d(-180.0, -90.0, 180.0, 90.0))
            .into_2d();

        // Estimate resolution based on extent (this would ideally come from COG metadata)
        let x_range = maxx - minx;
        let y_range = maxy - miny;
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
                        lower_bound: miny,
                        upper_bound: maxy,
                        resolution: estimated_resolution.max(0.0001),
                        uom_label: "deg".to_string(),
                    },
                    GridAxis {
                        axis_type: "RegularAxis".to_string(),
                        axis_label: "Long".to_string(),
                        lower_bound: minx,
                        upper_bound: maxx,
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
        collection: &Collection,
    ) -> AppResult<RangeType> {
        if collection.collection_type != "raster" {
            return Err(BadRequest {
                message: "Coverages only available for raster collections".to_string(),
            }.build());
        }

        // Get number of items to use as hint for band count
        // In a full implementation, we would read the COG metadata
        let mut tx = self.db.begin_as(username).await?;
        let item_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM spatialvault.items WHERE collection_id = $1")
                .bind(collection.id)
                .fetch_one(&mut *tx)
                .await?;

        // Default to single band description (would be enhanced with GDAL metadata)
        Ok(RangeType {
            range_type: "DataRecord".to_string(),
            field: vec![RangeField {
                field_type: "Quantity".to_string(),
                id: "band1".to_string(),
                name: "Band 1".to_string(),
                description: Some(format!("Raster data from {} items", item_count.0)),
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
        collection: &Collection,
        _params: &CoverageSubsetParams,
    ) -> AppResult<Vec<u8>> {
        if collection.collection_type != "raster" {
            return Err(BadRequest {
                message: "Coverages only available for raster collections".to_string(),
            }.build());
        }

        // Get the primary asset for the first item in the collection
        // In a full implementation, we would:
        // 1. Select items based on subsetting parameters
        // 2. Use GDAL to read and transform the COG data
        // 3. Apply any requested subsetting/resampling

        let mut tx = self.db.begin_as(username).await?;
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
        .fetch_optional(&mut *tx)
        .await?;

        match asset {
            Some((href,)) => {
                // For now, return a redirect hint in the error
                // A full implementation would use GDAL to read the COG
                Err(Internal { message: format!(
                    "Coverage data available at: {}. Direct access requires GDAL integration.",
                    href
                ) }.build())
            }
            None => Err(NotFound {
                message: "No raster data available for this collection".to_string(),
            }.build()),
        }
    }

    /// Get asset URLs for a collection (useful for clients that can read COGs directly)
    pub async fn get_collection_assets(
        &self,
        username: &str,
        collection: &Collection,
    ) -> AppResult<Vec<(String, String)>> {
        let mut tx = self.db.begin_as(username).await?;
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
        .fetch_all(&mut *tx)
        .await?;

        Ok(assets)
    }
}
