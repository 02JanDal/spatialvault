use bytes::Bytes;
use object_store::{ObjectStore, aws::AmazonS3Builder, path::Path};
use std::sync::Arc;

use crate::config::S3Config;
use crate::error::{AppError, AppResult};

pub struct S3Storage {
    store: Arc<dyn ObjectStore>,
    bucket: String,
}

impl S3Storage {
    pub fn new(config: &S3Config) -> AppResult<Self> {
        let mut builder = AmazonS3Builder::new().with_bucket_name(&config.bucket);

        if let Some(ref endpoint) = config.endpoint {
            builder = builder.with_endpoint(endpoint);
        }

        if let Some(ref region) = config.region {
            builder = builder.with_region(region);
        }

        if let Some(ref access_key_id) = config.access_key_id {
            builder = builder.with_access_key_id(access_key_id);
        }

        if let Some(ref secret_access_key) = config.secret_access_key {
            builder = builder.with_secret_access_key(secret_access_key);
        }

        let store = builder
            .build()
            .map_err(|e| AppError::Storage(format!("Failed to create S3 client: {}", e)))?;

        Ok(Self {
            store: Arc::new(store),
            bucket: config.bucket.clone(),
        })
    }

    /// Get an object from S3
    pub async fn get(&self, key: &str) -> AppResult<Bytes> {
        let path = Path::from(key);
        let result = self
            .store
            .get(&path)
            .await
            .map_err(|e| AppError::Storage(format!("Failed to get object: {}", e)))?;

        let bytes = result
            .bytes()
            .await
            .map_err(|e| AppError::Storage(format!("Failed to read object: {}", e)))?;

        Ok(bytes)
    }

    /// Put an object to S3
    pub async fn put(&self, key: &str, data: Bytes) -> AppResult<()> {
        let path = Path::from(key);
        self.store
            .put(&path, data.into())
            .await
            .map_err(|e| AppError::Storage(format!("Failed to put object: {}", e)))?;

        Ok(())
    }

    /// Delete an object from S3
    pub async fn delete(&self, key: &str) -> AppResult<()> {
        let path = Path::from(key);
        self.store
            .delete(&path)
            .await
            .map_err(|e| AppError::Storage(format!("Failed to delete object: {}", e)))?;

        Ok(())
    }

    /// Check if an object exists
    pub async fn exists(&self, key: &str) -> AppResult<bool> {
        let path = Path::from(key);
        match self.store.head(&path).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(AppError::Storage(format!("Failed to check object: {}", e))),
        }
    }

    /// Get object metadata (size, content-type, etc.)
    pub async fn head(&self, key: &str) -> AppResult<ObjectMeta> {
        let path = Path::from(key);
        let meta = self
            .store
            .head(&path)
            .await
            .map_err(|e| AppError::Storage(format!("Failed to get object metadata: {}", e)))?;

        Ok(ObjectMeta {
            size: meta.size,
            location: meta.location.to_string(),
            last_modified: meta.last_modified,
        })
    }

    /// List objects with a prefix
    pub async fn list(&self, prefix: &str) -> AppResult<Vec<String>> {
        use futures::StreamExt;

        let path = Path::from(prefix);
        let mut stream = self.store.list(Some(&path));
        let mut keys = Vec::new();

        while let Some(result) = stream.next().await {
            let meta =
                result.map_err(|e| AppError::Storage(format!("Failed to list objects: {}", e)))?;
            keys.push(meta.location.to_string());
        }

        Ok(keys)
    }

    /// Get a presigned URL for an object
    pub fn presigned_url(&self, key: &str, _expires_in_secs: u64) -> String {
        // object_store doesn't support presigned URLs directly
        // In production, we'd use the AWS SDK for this
        format!("s3://{}/{}", self.bucket, key)
    }

    /// Get the S3 URI for an object
    pub fn s3_uri(&self, key: &str) -> String {
        format!("s3://{}/{}", self.bucket, key)
    }
}

pub struct ObjectMeta {
    pub size: usize,
    pub location: String,
    pub last_modified: chrono::DateTime<chrono::Utc>,
}
