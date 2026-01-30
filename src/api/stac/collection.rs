use schemars::JsonSchema;
use serde::Serialize;

use crate::api::common::{Extent, Link};

/// STAC Collection
#[derive(Debug, Serialize, JsonSchema)]
pub struct StacCollection {
    #[serde(rename = "type")]
    pub collection_type: String,
    pub stac_version: String,
    pub stac_extensions: Vec<String>,
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub license: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extent: Option<Extent>,
    pub links: Vec<Link>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summaries: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets: Option<serde_json::Value>,
}

impl StacCollection {
    pub fn from_collection(
        collection: &crate::db::Collection,
        extent: Option<Extent>,
        base_url: &str,
    ) -> Self {
        use crate::api::common::{media_type, rel};

        let id = &collection.canonical_name;

        Self {
            collection_type: "Collection".to_string(),
            stac_version: "1.0.0".to_string(),
            stac_extensions: vec![],
            id: id.clone(),
            title: collection.title.clone(),
            description: collection.description.clone(),
            license: "proprietary".to_string(),
            extent,
            links: vec![
                Link::new(format!("{}/collections/{}", base_url, id), rel::SELF)
                    .with_type(media_type::JSON),
                Link::new(format!("{}/stac", base_url), rel::ROOT).with_type(media_type::JSON),
                Link::new(format!("{}/stac", base_url), rel::PARENT).with_type(media_type::JSON),
                Link::new(format!("{}/collections/{}/items", base_url, id), rel::ITEMS)
                    .with_type(media_type::GEOJSON),
            ],
            summaries: None,
            assets: None,
        }
    }
}
