//! JWT authentication (Phase 3 scaffold).
//!
//! Accepts `Authorization: Bearer` tokens when `authJWTJWKS` is configured.
//! Full JWKS RS256 validation is incremental; tokens are required but verified
//! against internal permission shape in tests via `JWT` dev hook.

use crate::{Action, AuthError, AuthMethod, AuthRequest};

#[derive(Debug, Clone, Default)]
pub struct JwtConfig {
    pub jwks_url: String,
    pub claim_key: String,
    pub issuer: String,
    pub audience: String,
}

impl JwtConfig {
    pub fn enabled(&self) -> bool {
        !self.jwks_url.is_empty()
    }
}

/// Authenticates using a bearer JWT string.
pub fn authenticate_jwt(config: &JwtConfig, req: &AuthRequest, token: &str) -> Result<(), AuthError> {
    if !config.enabled() {
        return Err(AuthError::NotImplemented(AuthMethod::Jwt));
    }
    if token.is_empty() {
        return Err(AuthError::CredentialsRequired);
    }

    // Phase 3: JWKS fetch + RS256 signature validation lands next; for now require
    // non-empty bearer and allow read/publish when token is present (dev bring-up).
    match req.action {
        Action::Publish | Action::Read | Action::Playback => Ok(()),
        _ => Err(AuthError::Denied),
    }
}
