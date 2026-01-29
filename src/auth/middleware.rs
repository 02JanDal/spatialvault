use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

use super::{AuthenticatedUser, OidcValidator};
use crate::error::AppError;

#[derive(Clone)]
pub struct AuthState {
    pub validator: Arc<OidcValidator>,
}

pub async fn auth_middleware(
    State(auth): State<AuthState>,
    mut request: Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    // Extract Authorization header
    let auth_header = request
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok());

    let token = match auth_header {
        Some(header) if header.starts_with("Bearer ") => &header[7..],
        _ => {
            return Err((
                StatusCode::UNAUTHORIZED,
                "Missing or invalid Authorization header".to_string(),
            ));
        }
    };

    // Validate token
    let claims = auth
        .validator
        .validate_token(token)
        .await
        .map_err(|e| match e {
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        })?;

    // Create authenticated user and insert into request extensions
    let user = AuthenticatedUser::from_claims(&claims);
    request.extensions_mut().insert(user);

    Ok(next.run(request).await)
}

/// Optional auth middleware - doesn't fail if no token present
pub async fn optional_auth_middleware(
    State(auth): State<AuthState>,
    mut request: Request,
    next: Next,
) -> Response {
    if let Some(auth_header) = request
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
    {
        if let Some(token) = auth_header.strip_prefix("Bearer ") {
            if let Ok(claims) = auth.validator.validate_token(token).await {
                let user = AuthenticatedUser::from_claims(&claims);
                request.extensions_mut().insert(user);
            }
        }
    }

    next.run(request).await
}

/// Extract authenticated user from request extensions
pub fn get_user(request: &Request) -> Option<&AuthenticatedUser> {
    request.extensions().get::<AuthenticatedUser>()
}
