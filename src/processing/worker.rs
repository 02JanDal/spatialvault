use base64::Engine;
use bytes::Bytes;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

use crate::api::processes::InputValue;
use crate::db::{Collection, Database};
use crate::error::{AppError, AppResult};
use crate::processing::{cog, copc};
use crate::services::{CollectionService, ItemService, ProcessService};
use crate::storage::S3Storage;

pub struct JobWorker {
    db: Arc<Database>,
    storage: Arc<S3Storage>,
    process_service: Arc<ProcessService>,
    item_service: Arc<ItemService>,
    collection_service: Arc<CollectionService>,
    temp_dir: PathBuf,
}

impl JobWorker {
    pub fn new(
        db: Arc<Database>,
        storage: Arc<S3Storage>,
        process_service: Arc<ProcessService>,
        item_service: Arc<ItemService>,
        collection_service: Arc<CollectionService>,
    ) -> Self {
        let temp_dir = std::env::temp_dir().join("spatialvault");
        std::fs::create_dir_all(&temp_dir).ok();

        Self {
            db,
            storage,
            process_service,
            item_service,
            collection_service,
            temp_dir,
        }
    }

    /// Start the background job worker
    pub async fn run(&self) -> AppResult<()> {
        tracing::info!("Starting job worker");

        loop {
            match self.poll_and_process_job().await {
                Ok(true) => {
                    // Processed a job, immediately check for more
                    continue;
                }
                Ok(false) => {
                    // No jobs available, wait before polling again
                    sleep(Duration::from_secs(5)).await;
                }
                Err(e) => {
                    tracing::error!("Job worker error: {}", e);
                    sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    /// Poll for a pending job and process it
    async fn poll_and_process_job(&self) -> AppResult<bool> {
        // Get next pending job (with row locking)
        let job: Option<(Uuid, String, String, serde_json::Value)> = sqlx::query_as(
            r#"
            UPDATE spatialvault.processes_jobs
            SET status = 'running', started = NOW(), updated = NOW()
            WHERE id = (
                SELECT id FROM spatialvault.processes_jobs
                WHERE status = 'accepted'
                ORDER BY created
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            )
            RETURNING id, process_id, owner, inputs
            "#,
        )
        .fetch_optional(self.db.pool())
        .await?;

        let (job_id, process_id, owner, inputs) = match job {
            Some(j) => j,
            None => return Ok(false),
        };

        tracing::info!("Processing job {} ({}) for user {}", job_id, process_id, owner);

        // Process based on type
        let result = match process_id.as_str() {
            "import-raster" => self.process_import_raster(job_id, &owner, &inputs).await,
            "import-pointcloud" => self.process_import_pointcloud(job_id, &owner, &inputs).await,
            _ => Err(AppError::Processing(format!(
                "Unknown process: {}",
                process_id
            ))),
        };

        match result {
            Ok(outputs) => {
                self.process_service
                    .set_job_outputs(job_id, &outputs)
                    .await?;
                tracing::info!("Job {} completed successfully", job_id);
            }
            Err(e) => {
                self.process_service
                    .update_job_status(job_id, "failed", Some(&e.to_string()), None)
                    .await?;
                tracing::error!("Job {} failed: {}", job_id, e);
            }
        }

        Ok(true)
    }

    async fn process_import_raster(
        &self,
        job_id: Uuid,
        owner: &str,
        inputs: &serde_json::Value,
    ) -> AppResult<serde_json::Value> {
        use crate::api::processes::import_raster::ImportRasterInputs;

        let inputs: ImportRasterInputs = serde_json::from_value(inputs.clone())?;

        // 1. Validate/get collection
        self.process_service
            .update_job_status(job_id, "running", Some("Validating collection"), Some(5))
            .await?;

        let collection = self
            .get_or_create_collection(owner, &inputs.collection, "raster")
            .await?;

        // 2. Get source file (download from URL or decode from base64)
        self.process_service
            .update_job_status(job_id, "running", Some("Retrieving source file"), Some(10))
            .await?;

        let source_path = self.get_input_file(&inputs.data, job_id, "tif").await?;

        // 3. Check if conversion is needed
        self.process_service
            .update_job_status(job_id, "running", Some("Checking file format"), Some(30))
            .await?;

        let is_already_cog = inputs.skip_if_cog && cog::is_cog(&source_path)?;
        let (final_path, converted) = if is_already_cog {
            tracing::info!("File is already a valid COG, skipping conversion");
            (source_path.clone(), false)
        } else {
            // 4. Convert to COG
            self.process_service
                .update_job_status(job_id, "running", Some("Converting to COG"), Some(40))
                .await?;

            let output_path = self.temp_dir.join(format!("{}.cog.tif", job_id));

            // Try conversion, fall back to pass-through if not implemented
            match cog::convert_to_cog(&source_path, &output_path).await {
                Ok(()) => (output_path, true),
                Err(e) => {
                    tracing::warn!("COG conversion not available: {}, using source file", e);
                    (source_path.clone(), false)
                }
            }
        };

        // 5. Extract metadata (bounds for geometry)
        self.process_service
            .update_job_status(job_id, "running", Some("Extracting metadata"), Some(60))
            .await?;

        let (geometry_wkt, srid) = self.extract_raster_bounds(&final_path).await?;

        // 6. Upload to S3
        self.process_service
            .update_job_status(job_id, "running", Some("Uploading to storage"), Some(70))
            .await?;

        let item_id = Uuid::new_v4();

        let s3_key = format!(
            "{}/{}/{}.tif",
            owner, collection.table_name, item_id
        );
        let file_data = tokio::fs::read(&final_path).await?;
        let file_size = file_data.len() as i64;
        self.storage.put(&s3_key, Bytes::from(file_data)).await?;
        let asset_href = self.storage.s3_uri(&s3_key);

        // 7. Create item and asset records
        self.process_service
            .update_job_status(job_id, "running", Some("Creating database records"), Some(90))
            .await?;

        let datetime = inputs
            .datetime
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc));

        let item = self
            .item_service
            .create_item(
                collection.id,
                &geometry_wkt,
                srid,
                datetime,
                inputs.properties.as_ref(),
            )
            .await?;

        let media_type = if converted {
            "image/tiff; application=geotiff; profile=cloud-optimized"
        } else {
            "image/tiff; application=geotiff"
        };

        self.item_service
            .create_asset(
                item.id,
                "data",
                &asset_href,
                Some(media_type),
                inputs.title.as_deref(),
                None,
                Some(&["data"]),
                Some(file_size),
                None,
            )
            .await?;

        // Cleanup temp files
        tokio::fs::remove_file(&source_path).await.ok();
        if converted && final_path != source_path {
            tokio::fs::remove_file(&final_path).await.ok();
        }

        Ok(serde_json::json!({
            "item_id": item.id.to_string(),
            "collection": inputs.collection,
            "asset_href": asset_href,
            "converted": converted
        }))
    }

    async fn process_import_pointcloud(
        &self,
        job_id: Uuid,
        owner: &str,
        inputs: &serde_json::Value,
    ) -> AppResult<serde_json::Value> {
        use crate::api::processes::import_pointcloud::ImportPointCloudInputs;

        let inputs: ImportPointCloudInputs = serde_json::from_value(inputs.clone())?;

        // 1. Validate/get collection
        self.process_service
            .update_job_status(job_id, "running", Some("Validating collection"), Some(5))
            .await?;

        let collection = self
            .get_or_create_collection(owner, &inputs.collection, "pointcloud")
            .await?;

        // 2. Get source file (download from URL or decode from base64)
        self.process_service
            .update_job_status(job_id, "running", Some("Retrieving source file"), Some(10))
            .await?;

        let source_path = self.get_input_file(&inputs.data, job_id, "laz").await?;

        // 3. Check if conversion is needed
        self.process_service
            .update_job_status(job_id, "running", Some("Checking file format"), Some(30))
            .await?;

        let is_already_copc = inputs.skip_if_copc && copc::is_copc(&source_path)?;
        let (final_path, converted) = if is_already_copc {
            tracing::info!("File is already a valid COPC, skipping conversion");
            (source_path.clone(), false)
        } else {
            // 4. Convert to COPC
            self.process_service
                .update_job_status(job_id, "running", Some("Converting to COPC"), Some(40))
                .await?;

            let output_path = self.temp_dir.join(format!("{}.copc.laz", job_id));

            match copc::convert_to_copc(&source_path, &output_path).await {
                Ok(()) => (output_path, true),
                Err(e) => {
                    tracing::warn!("COPC conversion not available: {}, using source file", e);
                    (source_path.clone(), false)
                }
            }
        };

        // 5. Extract metadata (bounds for geometry)
        self.process_service
            .update_job_status(job_id, "running", Some("Extracting metadata"), Some(60))
            .await?;

        let (geometry_wkt, srid) = self.extract_pointcloud_bounds(&final_path).await?;

        // 6. Upload to S3
        self.process_service
            .update_job_status(job_id, "running", Some("Uploading to storage"), Some(70))
            .await?;

        let item_id = Uuid::new_v4();

        let extension = if converted { "copc.laz" } else { "laz" };
        let s3_key = format!(
            "{}/{}/{}.{}",
            owner, collection.table_name, item_id, extension
        );
        let file_data = tokio::fs::read(&final_path).await?;
        let file_size = file_data.len() as i64;
        self.storage.put(&s3_key, Bytes::from(file_data)).await?;
        let asset_href = self.storage.s3_uri(&s3_key);

        // 7. Create item and asset records
        self.process_service
            .update_job_status(job_id, "running", Some("Creating database records"), Some(90))
            .await?;

        let datetime = inputs
            .datetime
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc));

        let item = self
            .item_service
            .create_item(
                collection.id,
                &geometry_wkt,
                srid,
                datetime,
                inputs.properties.as_ref(),
            )
            .await?;

        let media_type = "application/vnd.laszip+copc";

        self.item_service
            .create_asset(
                item.id,
                "data",
                &asset_href,
                Some(media_type),
                inputs.title.as_deref(),
                None,
                Some(&["data"]),
                Some(file_size),
                None,
            )
            .await?;

        // Cleanup temp files
        tokio::fs::remove_file(&source_path).await.ok();
        if converted && final_path != source_path {
            tokio::fs::remove_file(&final_path).await.ok();
        }

        Ok(serde_json::json!({
            "item_id": item.id.to_string(),
            "collection": inputs.collection,
            "asset_href": asset_href,
            "converted": converted
        }))
    }

    /// Get existing collection or create a new one
    async fn get_or_create_collection(
        &self,
        owner: &str,
        collection_name: &str,
        collection_type: &str,
    ) -> AppResult<Collection> {
        // Try to get existing collection
        if let Some(collection) = self
            .collection_service
            .get_collection(owner, collection_name)
            .await?
        {
            // Verify type matches
            if collection.collection_type != collection_type {
                return Err(AppError::BadRequest(format!(
                    "Collection '{}' exists but is type '{}', expected '{}'",
                    collection_name, collection.collection_type, collection_type
                )));
            }
            return Ok(collection.as_collection());
        }

        // Create new collection
        let canonical_name = if collection_name.contains(':') {
            collection_name.to_string()
        } else {
            format!("{}:{}", owner, collection_name)
        };

        self.collection_service
            .create_collection(
                owner,
                &canonical_name,
                owner,
                collection_name, // Use name as title
                None,
                collection_type,
                4326, // Default to WGS84
            )
            .await
    }

    /// Get input file - either decode from base64 or download from URL
    async fn get_input_file(
        &self,
        input: &InputValue,
        job_id: Uuid,
        default_extension: &str,
    ) -> AppResult<PathBuf> {
        match input {
            InputValue::Inline(inline) => {
                // Decode base64 and write to temp file
                let data = base64::engine::general_purpose::STANDARD
                    .decode(&inline.value)
                    .map_err(|e| AppError::BadRequest(format!("Invalid base64: {}", e)))?;

                // Determine extension from media type if available
                let extension = inline
                    .media_type
                    .as_ref()
                    .and_then(|mt| match mt.as_str() {
                        "image/tiff" | "image/geotiff" => Some("tif"),
                        "application/vnd.laszip" | "application/vnd.laszip+copc" => Some("laz"),
                        "application/vnd.las" => Some("las"),
                        _ => None,
                    })
                    .unwrap_or(default_extension);

                let local_path = self
                    .temp_dir
                    .join(format!("{}_source.{}", job_id, extension));
                tokio::fs::write(&local_path, &data).await?;
                tracing::info!("Wrote inline data ({} bytes) to {:?}", data.len(), local_path);
                Ok(local_path)
            }
            InputValue::Reference(reference) => {
                self.download_file(&reference.href, job_id).await
            }
        }
    }

    /// Download a file from URL (HTTP or S3)
    async fn download_file(&self, url: &str, job_id: Uuid) -> AppResult<PathBuf> {
        let extension = url
            .rsplit('/')
            .next()
            .and_then(|f| f.rsplit('.').next())
            .unwrap_or("bin");

        // Validate extension to prevent path traversal - must be alphanumeric only
        let safe_extension = if extension.chars().all(|c| c.is_ascii_alphanumeric()) && extension.len() <= 10 {
            extension
        } else {
            "bin"
        };

        let local_path = self.temp_dir.join(format!("{}_source.{}", job_id, safe_extension));

        if url.starts_with("s3://") {
            // Parse S3 URL: s3://bucket/key
            let path = url.strip_prefix("s3://").unwrap();
            let key = path.split_once('/').map(|(_, k)| k).unwrap_or(path);

            let data = self.storage.get(key).await?;
            tokio::fs::write(&local_path, &data).await?;
        } else if url.starts_with("http://") || url.starts_with("https://") {
            // HTTP download
            let response = reqwest::get(url)
                .await
                .map_err(|e| AppError::Processing(format!("Failed to download: {}", e)))?;

            if !response.status().is_success() {
                return Err(AppError::Processing(format!(
                    "Download failed with status: {}",
                    response.status()
                )));
            }

            let bytes = response
                .bytes()
                .await
                .map_err(|e| AppError::Processing(format!("Failed to read response: {}", e)))?;

            tokio::fs::write(&local_path, &bytes).await?;
        } else {
            return Err(AppError::BadRequest(format!(
                "Unsupported URL scheme: {}",
                url
            )));
        }

        tracing::info!("Downloaded {} to {:?}", url, local_path);
        Ok(local_path)
    }

    /// Extract bounds from raster file (returns WKT POLYGON and SRID)
    async fn extract_raster_bounds(&self, path: &PathBuf) -> AppResult<(String, i32)> {
        // Try to use GDAL for proper metadata extraction
        match cog::extract_raster_metadata(path).await {
            Ok(meta) => {
                let wkt = format!(
                    "POLYGON(({} {}, {} {}, {} {}, {} {}, {} {}))",
                    meta.bounds[0], meta.bounds[1], // minx, miny
                    meta.bounds[2], meta.bounds[1], // maxx, miny
                    meta.bounds[2], meta.bounds[3], // maxx, maxy
                    meta.bounds[0], meta.bounds[3], // minx, maxy
                    meta.bounds[0], meta.bounds[1], // close polygon
                );
                Ok((wkt, meta.srid))
            }
            Err(_) => {
                // Fallback: use a placeholder global extent
                tracing::warn!("Could not extract raster bounds, using placeholder");
                let wkt = "POLYGON((-180 -90, 180 -90, 180 90, -180 90, -180 -90))".to_string();
                Ok((wkt, 4326))
            }
        }
    }

    /// Extract bounds from point cloud file (returns WKT POLYGON and SRID)
    async fn extract_pointcloud_bounds(&self, path: &PathBuf) -> AppResult<(String, i32)> {
        match copc::extract_pointcloud_metadata(path).await {
            Ok(meta) => {
                // Use 2D bounds (ignore Z)
                let wkt = format!(
                    "POLYGON(({} {}, {} {}, {} {}, {} {}, {} {}))",
                    meta.bounds[0], meta.bounds[1], // minx, miny
                    meta.bounds[3], meta.bounds[1], // maxx, miny
                    meta.bounds[3], meta.bounds[4], // maxx, maxy
                    meta.bounds[0], meta.bounds[4], // minx, maxy
                    meta.bounds[0], meta.bounds[1], // close polygon
                );
                Ok((wkt, meta.srid))
            }
            Err(_) => {
                // Fallback: use a placeholder global extent
                tracing::warn!("Could not extract point cloud bounds, using placeholder");
                let wkt = "POLYGON((-180 -90, 180 -90, 180 90, -180 90, -180 -90))".to_string();
                Ok((wkt, 4326))
            }
        }
    }
}
