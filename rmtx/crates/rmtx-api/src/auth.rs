//! Control API authentication (Phase 2 scaffold).
//!
//! When `authMethod` is `internal` and `authInternalUsers` is empty, API auth is
//! skipped so the Control API remains usable without credentials (Phase 2 scaffold).
//! Once users are configured, requests must pass [`AuthManager::authenticate`] for
//! [`Action::Api`] (typically HTTP Basic auth).

use axum::{
    body::Body,
    extract::State,
    http::{header::AUTHORIZATION, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use rmtx_auth::{
    Action, AuthError, AuthInternalUser, AuthInternalUserPermission, AuthManager, AuthMethod,
    AuthRequest, JwtConfig,
};
use rmtx_conf::Conf;

use crate::error::ApiError;
use crate::state::AppState;

/// Builds an [`AuthManager`] and whether Control API auth should be enforced.
pub fn auth_from_conf(conf: &Conf) -> (AuthManager, bool) {
    let method = AuthMethod::parse(&conf.auth_method).unwrap_or(AuthMethod::Internal);
    let users: Vec<AuthInternalUser> = conf
        .auth_internal_users
        .iter()
        .filter_map(conf_user_to_auth)
        .collect();
    let api_auth_enabled = !(method == AuthMethod::Internal && users.is_empty());
    let auth = AuthManager::new(method, users).with_jwt(JwtConfig {
        jwks_url: conf.auth_jwt_jwks.clone(),
        claim_key: if conf.auth_jwt_claim_key.is_empty() {
            "mediamtx_permissions".into()
        } else {
            conf.auth_jwt_claim_key.clone()
        },
        issuer: conf.auth_jwt_issuer.clone(),
        audience: conf.auth_jwt_audience.clone(),
    });
    (auth, api_auth_enabled)
}

fn conf_user_to_auth(user: &rmtx_conf::AuthInternalUser) -> Option<AuthInternalUser> {
    let permissions = user
        .permissions
        .iter()
        .filter_map(|perm| {
            let action = parse_action(&perm.action)?;
            Some(AuthInternalUserPermission {
                action,
                path: perm.path.clone(),
            })
        })
        .collect();

    Some(AuthInternalUser {
        user: user.user.clone(),
        pass: user.pass.clone(),
        ips: user.ips.clone(),
        permissions,
    })
}

fn parse_action(value: &str) -> Option<Action> {
    match value {
        "publish" => Some(Action::Publish),
        "read" => Some(Action::Read),
        "playback" => Some(Action::Playback),
        "api" => Some(Action::Api),
        "metrics" => Some(Action::Metrics),
        "pprof" => Some(Action::Pprof),
        _ => None,
    }
}

/// Axum middleware: enforces internal auth for Control API when configured.
pub async fn require_api_auth(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if !state.api_auth_enabled() {
        return next.run(req).await;
    }

    let ip = client_ip(req.headers());
    let creds = basic_credentials(req.headers().get(AUTHORIZATION));
    let token = bearer_token(req.headers().get(AUTHORIZATION));
    let auth_req = AuthRequest {
        user: creds.user,
        pass: creds.pass,
        token,
        ip,
        path: String::new(),
        action: Action::Api,
        protocol: "http".to_owned(),
    };

    match state.auth().authenticate(&auth_req) {
        Ok(()) => next.run(req).await,
        Err(AuthError::CredentialsRequired) => (
            StatusCode::UNAUTHORIZED,
            [(axum::http::header::WWW_AUTHENTICATE, "Basic realm=\"rmtx\"")],
            Json(ApiError::new("credentials required")),
        )
            .into_response(),
        Err(AuthError::Denied) => (
            StatusCode::UNAUTHORIZED,
            Json(ApiError::new("authentication failed")),
        )
            .into_response(),
        Err(AuthError::NotImplemented(method)) => (
            StatusCode::NOT_IMPLEMENTED,
            Json(ApiError::new(format!(
                "{method:?} authentication is not implemented yet"
            ))),
        )
            .into_response(),
    }
}

struct BasicCredentials {
    user: String,
    pass: String,
}

fn bearer_token(header: Option<&axum::http::HeaderValue>) -> String {
    let Some(header) = header else {
        return String::new();
    };
    let Ok(value) = header.to_str() else {
        return String::new();
    };
    value
        .strip_prefix("Bearer ")
        .unwrap_or("")
        .trim()
        .to_owned()
}

fn basic_credentials(header: Option<&axum::http::HeaderValue>) -> BasicCredentials {
    let Some(header) = header else {
        return BasicCredentials {
            user: String::new(),
            pass: String::new(),
        };
    };

    let Ok(value) = header.to_str() else {
        return BasicCredentials {
            user: String::new(),
            pass: String::new(),
        };
    };

    let Some(encoded) = value.strip_prefix("Basic ") else {
        return BasicCredentials {
            user: String::new(),
            pass: String::new(),
        };
    };

    let Ok(decoded) = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
    else {
        return BasicCredentials {
            user: String::new(),
            pass: String::new(),
        };
    };

    let Ok(text) = String::from_utf8(decoded) else {
        return BasicCredentials {
            user: String::new(),
            pass: String::new(),
        };
    };

    match text.split_once(':') {
        Some((user, pass)) => BasicCredentials {
            user: user.to_owned(),
            pass: pass.to_owned(),
        },
        None => BasicCredentials {
            user: text,
            pass: String::new(),
        },
    }
}

fn client_ip(headers: &axum::http::HeaderMap) -> String {
    if let Some(value) = headers
        .get("X-Real-IP")
        .or_else(|| headers.get("X-Forwarded-For"))
        .and_then(|v| v.to_str().ok())
    {
        return value.split(',').next().unwrap_or(value).trim().to_owned();
    }
    "127.0.0.1".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmtx_conf::AuthInternalUserPermission as ConfPermission;

    #[test]
    fn empty_users_skip_api_auth() {
        let conf = Conf::default();
        let (_, enabled) = auth_from_conf(&conf);
        assert!(!enabled);
    }

    #[test]
    fn configured_users_enable_api_auth() {
        let mut conf = Conf::default();
        conf.auth_internal_users.push(rmtx_conf::AuthInternalUser {
            user: "admin".into(),
            pass: "secret".into(),
            ips: vec!["127.0.0.1/32".into()],
            permissions: vec![ConfPermission {
                action: "api".into(),
                path: String::new(),
            }],
        });
        let (auth, enabled) = auth_from_conf(&conf);
        assert!(enabled);
        auth.authenticate(&AuthRequest {
            user: "admin".into(),
            pass: "secret".into(),
            token: String::new(),
            ip: "127.0.0.1".into(),
            path: String::new(),
            action: Action::Api,
            protocol: "http".into(),
        })
        .unwrap();
    }
}
