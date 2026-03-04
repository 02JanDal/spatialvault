use std::path::Path;

use crate::db::{Collection, Database};
use crate::error::AppResult;
use crate::processing::vector::{VectorFeature, VectorImporter};
use crate::services::CollectionService;

/// Result of a vector file import
pub struct ImportResult {
    pub features_imported: usize,
    pub features_failed: usize,
    pub source_crs: i32,
    pub target_crs: i32,
}

/// Data extracted from a vector file via GDAL (all Send-safe).
struct VectorFileData {
    source_crs: i32,
    batches: Vec<Vec<VectorFeature>>,
}

/// Import features from a vector file into a collection.
///
/// This is the shared core logic used by both the async worker and the
/// synchronous handler path (when S3 is not configured).
///
/// GDAL types are not Send, so all GDAL operations happen inside
/// `spawn_blocking`. The resulting feature batches (plain data) are
/// then inserted into the database asynchronously.
pub async fn import_vector_file(
    db: &Database,
    collection_service: &CollectionService,
    file_path: &Path,
    owner: &str,
    collection_id: &str,
) -> AppResult<ImportResult> {
    // Read all features from the vector file in a blocking task
    let path_buf = file_path.to_path_buf();
    let file_data: VectorFileData = tokio::task::spawn_blocking(move || -> AppResult<_> {
        let mut importer = VectorImporter::open(&path_buf)?;
        let source_crs = importer.get_source_crs()?;
        let total_features = importer.feature_count()?;

        tracing::info!(
            "Reading {} features from {} (EPSG:{})",
            total_features,
            path_buf.display(),
            source_crs
        );

        let batch_size = 1000;
        let mut batches = Vec::new();
        let mut offset = 0;

        loop {
            let batch = importer.read_features_batch(offset, batch_size)?;
            if batch.is_empty() {
                break;
            }
            offset += batch.len();
            batches.push(batch);
        }

        Ok(VectorFileData {
            source_crs,
            batches,
        })
    })
    .await
    .unwrap()?;

    let source_crs = file_data.source_crs;

    // Get collection and storage CRS
    let collection_with_crs = collection_service
        .get_collection(owner, collection_id)
        .await?;

    let collection = collection_with_crs.as_collection();
    let storage_crs = collection_with_crs.storage_crs;

    // Insert features into the database
    let mut imported = 0;
    let mut failed = 0;

    for batch in file_data.batches {
        let mut tx = db.pool().begin().await?;

        for feature in &batch {
            match insert_feature(
                &mut tx,
                collection_service,
                &collection,
                feature,
                source_crs,
                storage_crs,
            )
            .await
            {
                Ok(_) => imported += 1,
                Err(e) => {
                    failed += 1;
                    tracing::warn!("Failed to import feature: {}", e);
                }
            }
        }

        tx.commit().await?;
    }

    tracing::info!(
        "Import complete: {} imported, {} failed",
        imported,
        failed,
    );

    Ok(ImportResult {
        features_imported: imported,
        features_failed: failed,
        source_crs,
        target_crs: storage_crs,
    })
}

/// Insert a single feature into the collection
async fn insert_feature(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    collection_service: &CollectionService,
    collection: &Collection,
    feature: &VectorFeature,
    source_crs: i32,
    storage_crs: i32,
) -> AppResult<()> {
    let transform_fn = if source_crs == storage_crs {
        format!("ST_GeomFromText($1, {})", storage_crs)
    } else {
        format!(
            "ST_Transform(ST_GeomFromText($1, {}), {})",
            source_crs, storage_crs
        )
    };

    let quoted_schema = quote_ident(&collection.schema_name);
    let quoted_table = quote_ident(&collection.table_name);

    let user_columns = collection_service
        .get_user_columns(&mut **tx, collection)
        .await?;

    if user_columns.is_empty() {
        let query = format!(
            "INSERT INTO {quoted_schema}.{quoted_table} (geometry) VALUES ({transform_fn})",
        );
        sqlx::query(&query)
            .bind(&feature.geometry_wkt)
            .execute(&mut **tx)
            .await?;
    } else {
        let col_names: Vec<String> = user_columns
            .iter()
            .map(|c| quote_ident(&c.name))
            .collect();
        let col_refs: Vec<String> = user_columns
            .iter()
            .map(|c| format!("r.{}", quote_ident(&c.name)))
            .collect();

        let query = format!(
            "INSERT INTO {quoted_schema}.{quoted_table} (geometry, {cols}) \
             SELECT {transform_fn}, {col_refs} \
             FROM jsonb_populate_record(NULL::{quoted_schema}.{quoted_table}, $2) r",
            cols = col_names.join(", "),
            col_refs = col_refs.join(", "),
        );
        sqlx::query(&query)
            .bind(&feature.geometry_wkt)
            .bind(&feature.properties)
            .execute(&mut **tx)
            .await?;
    }

    Ok(())
}

/// Quote a PostgreSQL identifier (schema or table name)
pub fn quote_ident(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}
