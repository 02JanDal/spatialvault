use std::sync::Arc;
use uuid::Uuid;

use crate::db::{Asset, Database, Item};
use crate::error::AppResult;

pub struct ItemService {
    db: Arc<Database>,
}

impl ItemService {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Create a new item in a collection
    pub async fn create_item(
        &self,
        collection_id: Uuid,
        geometry_wkt: &str,
        srid: i32,
        datetime: Option<chrono::DateTime<chrono::Utc>>,
        properties: Option<&serde_json::Value>,
    ) -> AppResult<Item> {
        let item: Item = sqlx::query_as(&format!(
            r#"
                INSERT INTO spatialvault.items
                (collection_id, geometry, datetime, properties)
                VALUES ($1, ST_GeomFromText($2, {}), $3, $4)
                RETURNING id, collection_id, datetime, properties, version, created_at, updated_at
                "#,
            srid
        ))
        .bind(collection_id)
        .bind(geometry_wkt)
        .bind(datetime)
        .bind(properties)
        .fetch_one(self.db.pool())
        .await?;

        Ok(item)
    }

    /// Create a new asset for an item
    pub async fn create_asset(
        &self,
        item_id: Uuid,
        key: &str,
        href: &str,
        media_type: Option<&str>,
        title: Option<&str>,
        description: Option<&str>,
        roles: Option<&[&str]>,
        file_size: Option<i64>,
        extra_fields: Option<&serde_json::Value>,
    ) -> AppResult<Asset> {
        let roles_vec: Option<Vec<String>> =
            roles.map(|r| r.iter().map(|s| s.to_string()).collect());

        let asset: Asset = sqlx::query_as(
            r#"
            INSERT INTO spatialvault.assets
            (item_id, key, href, type, title, description, roles, file_size, extra_fields)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING id, item_id, key, href, type, title, description, roles, file_size, extra_fields, created_at
            "#,
        )
        .bind(item_id)
        .bind(key)
        .bind(href)
        .bind(media_type)
        .bind(title)
        .bind(description)
        .bind(roles_vec)
        .bind(file_size)
        .bind(extra_fields)
        .fetch_one(self.db.pool())
        .await?;

        Ok(asset)
    }
}
