use aide::{
    axum::{
        ApiRouter,
        routing::{delete_with, get_with, post_with},
    },
    transform::TransformOperation,
};
use axum::{
    Json,
    extract::{Extension, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::auth::AuthenticatedUser;
use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::services::CollectionService;

/// Share permission level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PermissionLevel {
    Read,
    Write,
}

impl PermissionLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            PermissionLevel::Read => "read",
            PermissionLevel::Write => "write",
        }
    }
}

/// A share entry for a collection
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ShareEntry {
    /// The principal (user or group name)
    pub principal: String,
    /// Whether this is a user or group
    pub principal_type: String,
    /// Permission level
    pub permission: PermissionLevel,
}

/// Response listing all shares for a collection
#[derive(Debug, Serialize, JsonSchema)]
pub struct SharesResponse {
    pub collection_id: String,
    pub shares: Vec<ShareEntry>,
}

/// Request to add a share
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddShareRequest {
    /// The principal to share with (user or group name)
    pub principal: String,
    /// Whether this is a "user" or "group"
    #[serde(default = "default_principal_type")]
    pub principal_type: String,
    /// Permission level: "read" or "write"
    pub permission: PermissionLevel,
}

fn default_principal_type() -> String {
    "user".to_string()
}

/// Path parameters for collection sharing endpoints
#[aide::axum::typed_path]
#[typed_path("/collections/{collection_id}/sharing")]
pub struct CollectionSharingPath {
    /// The collection identifier
    pub collection_id: String,
}

pub async fn list_shares(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State(service): State<Arc<CollectionService>>,
    path: CollectionSharingPath,
) -> Result<Response, AppError> {
    let collection_id = path.collection_id;
    // Check for alias redirect (only if no active collection with this exact name exists)
    if let Some(new_name) = service.check_alias_redirect(&collection_id).await? {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::LOCATION,
            format!("{}/collections/{}/sharing", config.base_url, new_name)
                .parse()
                .map_err(|_| AppError::Internal("Invalid redirect URL".to_string()))?,
        );
        return Ok((StatusCode::TEMPORARY_REDIRECT, headers).into_response());
    }

    let shares = service.list_shares(&user.username, &collection_id).await?;

    Ok(Json(SharesResponse {
        collection_id,
        shares,
    })
    .into_response())
}

fn list_shares_docs(op: TransformOperation) -> TransformOperation {
    op.summary("List shares")
        .description("Returns all sharing entries for a collection")
        .tag("Sharing")
        .response_with::<200, Json<SharesResponse>, _>(|res| {
            res.description("List of shares for the collection")
        })
}

pub async fn add_share(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State(service): State<Arc<CollectionService>>,
    path: CollectionSharingPath,
    Json(request): Json<AddShareRequest>,
) -> Result<Response, AppError> {
    let collection_id = path.collection_id;
    // Check for alias redirect (only if no active collection with this exact name exists)
    if let Some(new_name) = service.check_alias_redirect(&collection_id).await? {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::LOCATION,
            format!("{}/collections/{}/sharing", config.base_url, new_name)
                .parse()
                .map_err(|_| AppError::Internal("Invalid redirect URL".to_string()))?,
        );
        return Ok((StatusCode::TEMPORARY_REDIRECT, headers).into_response());
    }

    service
        .add_share(
            &user.username,
            &collection_id,
            &request.principal,
            &request.principal_type,
            request.permission,
        )
        .await?;

    Ok(StatusCode::CREATED.into_response())
}

fn add_share_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Add share")
        .description("Shares a collection with a user or group at a specified permission level")
        .tag("Sharing")
        .response_with::<201, (), _>(|res| res.description("Share added successfully"))
        .response_with::<400, (), _>(|res| res.description("Invalid request"))
        .response_with::<403, (), _>(|res| res.description("Permission denied"))
}

/// Path parameters for removing a specific share
#[aide::axum::typed_path]
#[typed_path("/collections/{collection_id}/sharing/{principal}")]
pub struct SharePrincipalPath {
    /// The collection identifier
    pub collection_id: String,
    /// The principal (user or group) to remove
    pub principal: String,
}

pub async fn remove_share(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State(service): State<Arc<CollectionService>>,
    path: SharePrincipalPath,
) -> Result<Response, AppError> {
    let collection_id = path.collection_id;
    // Check for alias redirect (only if no active collection with this exact name exists)
    if let Some(new_name) = service.check_alias_redirect(&collection_id).await? {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::LOCATION,
            format!(
                "{}/collections/{}/sharing/{}",
                config.base_url, new_name, path.principal
            )
            .parse()
            .map_err(|_| AppError::Internal("Invalid redirect URL".to_string()))?,
        );
        return Ok((StatusCode::TEMPORARY_REDIRECT, headers).into_response());
    }

    let principal = path.principal;
    service
        .remove_share(&user.username, &collection_id, &principal)
        .await?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

fn remove_share_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Remove share")
        .description("Removes a sharing entry for a principal from a collection")
        .tag("Sharing")
        .response_with::<204, (), _>(|res| res.description("Share removed"))
        .response_with::<404, (), _>(|res| res.description("Share not found"))
}

pub fn routes(service: Arc<CollectionService>) -> ApiRouter {
    ApiRouter::new()
        .api_route(
            "/collections/{collection_id}/sharing",
            get_with(list_shares, list_shares_docs).post_with(add_share, add_share_docs),
        )
        .api_route(
            "/collections/{collection_id}/sharing/{principal}",
            delete_with(remove_share, remove_share_docs),
        )
        .with_state(service)
}
