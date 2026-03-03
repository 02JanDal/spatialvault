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
    pub dev_auth: bool,
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

    // Dev-mode: allow "Authorization: User <username>" to bypass OIDC
    if auth.dev_auth {
        if let Some(username) = auth_header.and_then(|h| h.strip_prefix("User ")) {
            let user = AuthenticatedUser {
                username: username.to_string(),
                subject: username.to_string(),
                groups: vec![],
            };
            request.extensions_mut().insert(user);
            return Ok(next.run(request).await);
        }
    }

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
            AppError::Unauthorized { message, .. } => (StatusCode::UNAUTHORIZED, message),
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
        // Dev-mode: allow "Authorization: User <username>" to bypass OIDC
        if auth.dev_auth {
            if let Some(username) = auth_header.strip_prefix("User ") {
                let user = AuthenticatedUser {
                    username: username.to_string(),
                    subject: username.to_string(),
                    groups: vec![],
                };
                request.extensions_mut().insert(user);
                return next.run(request).await;
            }
        }

        if let Some(token) = auth_header.strip_prefix("Bearer ") {
            if let Ok(claims) = auth.validator.validate_token(token).await {
                let user = AuthenticatedUser::from_claims(&claims);
                request.extensions_mut().insert(user);
            }
        }
    }

    next.run(request).await
}

/// No-auth middleware that injects a default anonymous user on all requests.
/// Used when `auth.disabled = true` to allow external validators (e.g. STAC API
/// validator, OGC CITE) to access all endpoints without OIDC tokens.
pub async fn no_auth_middleware(mut request: Request, next: Next) -> Response {
    let user = AuthenticatedUser {
        username: "anonymous".to_string(),
        subject: "anonymous".to_string(),
        groups: vec![],
    };
    request.extensions_mut().insert(user);
    next.run(request).await
}

/// Extract authenticated user from request extensions
pub fn get_user(request: &Request) -> Option<&AuthenticatedUser> {
    request.extensions().get::<AuthenticatedUser>()
}
