use std::sync::Arc;
use uuid::Uuid;

use crate::db::{Asset, Database, Item};
use crate::error::{AppError, AppResult};

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
        let item: Item = sqlx::query_as(
            &format!(
                r#"
                INSERT INTO spatialvault.items
                (collection_id, geometry, datetime, properties)
                VALUES ($1, ST_GeomFromText($2, {}), $3, $4)
                RETURNING id, collection_id, datetime, properties, version, created_at, updated_at
                "#,
                srid
            ),
        )
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
        let roles_vec: Option<Vec<String>> = roles.map(|r| r.iter().map(|s| s.to_string()).collect());

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

    /// Get an item by ID
    pub async fn get_item(&self, item_id: Uuid) -> AppResult<Option<Item>> {
        let item: Option<Item> = sqlx::query_as(
            r#"
            SELECT id, collection_id, datetime, properties, version, created_at, updated_at
            FROM spatialvault.items
            WHERE id = $1
            "#,
        )
        .bind(item_id)
        .fetch_optional(self.db.pool())
        .await?;

        Ok(item)
    }

    /// Get item with geometry as GeoJSON
    pub async fn get_item_with_geometry(&self, item_id: Uuid) -> AppResult<Option<ItemWithGeometry>> {
        let item: Option<ItemWithGeometry> = sqlx::query_as(
            r#"
            SELECT
                id,
                collection_id,
                ST_AsGeoJSON(geometry)::jsonb as geometry,
                datetime,
                properties,
                version,
                created_at,
                updated_at
            FROM spatialvault.items
            WHERE id = $1
            "#,
        )
        .bind(item_id)
        .fetch_optional(self.db.pool())
        .await?;

        Ok(item)
    }

    /// List items in a collection
    pub async fn list_items(
        &self,
        collection_id: Uuid,
        limit: u32,
        offset: u32,
    ) -> AppResult<Vec<ItemWithGeometry>> {
        let items: Vec<ItemWithGeometry> = sqlx::query_as(
            r#"
            SELECT
                id,
                collection_id,
                ST_AsGeoJSON(geometry)::jsonb as geometry,
                datetime,
                properties,
                version,
                created_at,
                updated_at
            FROM spatialvault.items
            WHERE collection_id = $1
            ORDER BY datetime DESC NULLS LAST, created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(collection_id)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(self.db.pool())
        .await?;

        Ok(items)
    }

    /// Get assets for an item
    pub async fn get_item_assets(&self, item_id: Uuid) -> AppResult<Vec<Asset>> {
        let assets: Vec<Asset> = sqlx::query_as(
            r#"
            SELECT id, item_id, key, href, type, title, description, roles, file_size, extra_fields, created_at
            FROM spatialvault.assets
            WHERE item_id = $1
            ORDER BY key
            "#,
        )
        .bind(item_id)
        .fetch_all(self.db.pool())
        .await?;

        Ok(assets)
    }

    /// Delete an item and its assets
    pub async fn delete_item(&self, item_id: Uuid) -> AppResult<()> {
        // Assets are deleted via CASCADE
        sqlx::query("DELETE FROM spatialvault.items WHERE id = $1")
            .bind(item_id)
            .execute(self.db.pool())
            .await?;

        Ok(())
    }

    /// Update item properties
    pub async fn update_item(
        &self,
        item_id: Uuid,
        expected_version: Option<i64>,
        datetime: Option<chrono::DateTime<chrono::Utc>>,
        properties: Option<&serde_json::Value>,
    ) -> AppResult<Item> {
        // If expected_version is provided, include version check in WHERE clause
        let item: Option<Item> = if let Some(version) = expected_version {
            sqlx::query_as(
                r#"
                UPDATE spatialvault.items
                SET
                    datetime = COALESCE($3, datetime),
                    properties = COALESCE($4, properties),
                    version = version + 1,
                    updated_at = NOW()
                WHERE id = $1 AND version = $2
                RETURNING id, collection_id, datetime, properties, version, created_at, updated_at
                "#,
            )
            .bind(item_id)
            .bind(version)
            .bind(datetime)
            .bind(properties)
            .fetch_optional(self.db.pool())
            .await?
        } else {
            sqlx::query_as(
                r#"
                UPDATE spatialvault.items
                SET
                    datetime = COALESCE($2, datetime),
                    properties = COALESCE($3, properties),
                    version = version + 1,
                    updated_at = NOW()
                WHERE id = $1
                RETURNING id, collection_id, datetime, properties, version, created_at, updated_at
                "#,
            )
            .bind(item_id)
            .bind(datetime)
            .bind(properties)
            .fetch_optional(self.db.pool())
            .await?
        };

        item.ok_or_else(|| {
            if expected_version.is_some() {
                AppError::PreconditionFailed("Item has been modified or does not exist".to_string())
            } else {
                AppError::NotFound("Item not found".to_string())
            }
        })
    }
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ItemWithGeometry {
    pub id: Uuid,
    pub collection_id: Uuid,
    pub geometry: serde_json::Value,
    pub datetime: Option<chrono::DateTime<chrono::Utc>>,
    pub properties: Option<serde_json::Value>,
    pub version: i64,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_item_with_geometry_serialization() {
        // ItemWithGeometry should be serializable
        let item = super::ItemWithGeometry {
            id: uuid::Uuid::new_v4(),
            collection_id: uuid::Uuid::new_v4(),
            geometry: serde_json::json!({
                "type": "Polygon",
                "coordinates": [[[-180, -90], [180, -90], [180, 90], [-180, 90], [-180, -90]]]
            }),
            datetime: Some(chrono::Utc::now()),
            properties: Some(serde_json::json!({"test": "value"})),
            version: 1,
            created_at: Some(chrono::Utc::now()),
            updated_at: Some(chrono::Utc::now()),
        };

        let json = serde_json::to_string(&item);
        assert!(json.is_ok());
    }
}
