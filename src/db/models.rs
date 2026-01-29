use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Collection {
    pub id: Uuid,
    pub canonical_name: String,
    pub owner: String,
    pub schema_name: String,
    pub table_name: String,
    pub collection_type: String,
    pub title: String,
    pub description: Option<String>,
    pub version: i64,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollectionType {
    Vector,
    Raster,
    PointCloud,
}

impl CollectionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CollectionType::Vector => "vector",
            CollectionType::Raster => "raster",
            CollectionType::PointCloud => "pointcloud",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "vector" => Some(CollectionType::Vector),
            "raster" => Some(CollectionType::Raster),
            "pointcloud" => Some(CollectionType::PointCloud),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CollectionAlias {
    pub old_name: String,
    pub new_name: String,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Item {
    pub id: Uuid,
    pub collection_id: Uuid,
    pub datetime: Option<DateTime<Utc>>,
    pub properties: Option<serde_json::Value>,
    pub version: i64,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Asset {
    pub id: Uuid,
    pub item_id: Uuid,
    pub key: String,
    pub href: String,
    #[sqlx(rename = "type")]
    pub media_type: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub roles: Option<Vec<String>>,
    pub file_size: Option<i64>,
    pub extra_fields: Option<serde_json::Value>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProcessJob {
    pub id: Uuid,
    pub process_id: String,
    pub status: String,
    pub owner: String,
    #[sqlx(rename = "type")]
    pub job_type: Option<String>,
    pub message: Option<String>,
    pub progress: Option<i32>,
    pub inputs: Option<serde_json::Value>,
    pub outputs: Option<serde_json::Value>,
    pub created: Option<DateTime<Utc>>,
    pub started: Option<DateTime<Utc>>,
    pub finished: Option<DateTime<Utc>>,
    pub updated: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Accepted,
    Running,
    Successful,
    Failed,
    Dismissed,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Accepted => "accepted",
            JobStatus::Running => "running",
            JobStatus::Successful => "successful",
            JobStatus::Failed => "failed",
            JobStatus::Dismissed => "dismissed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "accepted" => Some(JobStatus::Accepted),
            "running" => Some(JobStatus::Running),
            "successful" => Some(JobStatus::Successful),
            "failed" => Some(JobStatus::Failed),
            "dismissed" => Some(JobStatus::Dismissed),
            _ => None,
        }
    }
}
