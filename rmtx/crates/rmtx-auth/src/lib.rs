//! Internal, HTTP, and JWT authentication.
//!
//! Go counterpart: `internal/auth`.
//!
//! Phase 1 implements **internal** auth only. HTTP and JWT variants are wired in the
//! type system but return [`AuthError::NotImplemented`] until Phase 2.
//!
//! ## Wiring (future)
//!
//! Control API and protocol servers build an [`AuthRequest`] from the incoming
//! connection (credentials, client IP, path name, action, protocol) and call
//! [`AuthManager::authenticate`] before accepting publish/read/API access:
//!
//! ```ignore
//! let req = AuthRequest {
//!     user: creds.user,
//!     pass: creds.pass,
//!     ip: peer_ip,
//!     path: path_name,
//!     action: Action::Publish,
//!     protocol: "rtsp".into(),
//! };
//! auth.authenticate(&req)?;
//! ```
//!
//! `rmtx` process startup will construct [`AuthManager::new`] from loaded config
//! (`authMethod`, `authInternalUsers`) and share it with `rmtx-api` and server crates.

#![forbid(unsafe_code)]

use std::net::IpAddr;

use thiserror::Error;

mod jwt;

use jwt::authenticate_jwt;

pub use jwt::JwtConfig;

/// Authentication backend selected in config (`authMethod`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethod {
    Internal,
    Http,
    Jwt,
}

impl AuthMethod {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "internal" => Some(Self::Internal),
            "http" => Some(Self::Http),
            "jwt" => Some(Self::Jwt),
            _ => None,
        }
    }
}

/// Protected operation (maps to Go `conf.AuthAction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Publish,
    Read,
    Playback,
    Api,
    Metrics,
    Pprof,
}

/// One permission entry from `authInternalUsers[].permissions`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthInternalUserPermission {
    pub action: Action,
    /// Empty string matches any path; `~regex` matches via regular expression.
    pub path: String,
}

/// Internal user entry from config (`authInternalUsers`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthInternalUser {
    pub user: String,
    pub pass: String,
    /// Optional CIDR filters (e.g. `127.0.0.1/32`). Empty allows any IP.
    pub ips: Vec<String>,
    pub permissions: Vec<AuthInternalUserPermission>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthRequest {
    pub user: String,
    pub pass: String,
    /// Bearer JWT when `authMethod: jwt`.
    pub token: String,
    pub ip: String,
    pub path: String,
    pub action: Action,
    pub protocol: String,
}

/// Authentication failure modes.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuthError {
    #[error("authentication failed")]
    Denied,
    #[error("credentials required")]
    CredentialsRequired,
    #[error("{0:?} authentication is not implemented yet")]
    NotImplemented(AuthMethod),
}

/// Authentication manager (maps to Go `auth.Manager`).
#[derive(Debug, Clone)]
pub struct AuthManager {
    method: AuthMethod,
    internal_users: Vec<AuthInternalUser>,
    jwt: JwtConfig,
}

impl AuthManager {
    /// Builds a manager for the given method and internal user list.
    ///
    /// When `method` is [`AuthMethod::Internal`] and `internal_users` is empty,
    /// publish and read are allowed without credential checks (Phase 1 default).
    pub fn new(method: AuthMethod, internal_users: Vec<AuthInternalUser>) -> Self {
        Self {
            method,
            internal_users,
            jwt: JwtConfig::default(),
        }
    }

    /// Attaches JWT settings from config (`authJWT*` fields).
    pub fn with_jwt(mut self, jwt: JwtConfig) -> Self {
        self.jwt = jwt;
        self
    }

    /// Authenticates `req`. Returns `Ok(())` when access is granted.
    pub fn authenticate(&self, req: &AuthRequest) -> Result<(), AuthError> {
        match self.method {
            AuthMethod::Internal => self.authenticate_internal(req),
            AuthMethod::Http => Err(AuthError::NotImplemented(AuthMethod::Http)),
            AuthMethod::Jwt => authenticate_jwt(&self.jwt, req, &req.token),
        }
    }

    fn authenticate_internal(&self, req: &AuthRequest) -> Result<(), AuthError> {
        if self.internal_users.is_empty() {
            return match req.action {
                Action::Publish | Action::Read => Ok(()),
                _ => Err(AuthError::Denied),
            };
        }

        let mut saw_matching_permission = false;
        let mut credentials_required = false;

        for user in &self.internal_users {
            if !user.ips.is_empty() && !user.ips.iter().any(|cidr| ip_in_cidr(&req.ip, cidr)) {
                continue;
            }
            if !permission_matches(&user.permissions, req) {
                continue;
            }
            saw_matching_permission = true;

            if user.user == "any" {
                return Ok(());
            }

            if req.user.is_empty() && req.pass.is_empty() {
                credentials_required = true;
                continue;
            }

            if user.user == req.user && user.pass == req.pass {
                return Ok(());
            }
        }

        if saw_matching_permission && credentials_required {
            Err(AuthError::CredentialsRequired)
        } else {
            Err(AuthError::Denied)
        }
    }
}

fn permission_matches(perms: &[AuthInternalUserPermission], req: &AuthRequest) -> bool {
    perms.iter().any(|perm| {
        if perm.action != req.action {
            return false;
        }

        match req.action {
            Action::Publish | Action::Read | Action::Playback => {
                path_matches(&perm.path, &req.path)
            }
            Action::Api | Action::Metrics | Action::Pprof => true,
        }
    })
}

fn path_matches(perm_path: &str, req_path: &str) -> bool {
    if perm_path.is_empty() {
        return true;
    }

    if let Some(pattern) = perm_path.strip_prefix('~') {
        return regex::Regex::new(pattern)
            .map(|re| re.is_match(req_path))
            .unwrap_or(false);
    }

    perm_path == req_path
}

fn ip_in_cidr(ip: &str, cidr: &str) -> bool {
    let Ok(addr) = ip.parse::<IpAddr>() else {
        return false;
    };

    let Some((network, prefix_len)) = parse_cidr(cidr) else {
        return false;
    };

    match (addr, network) {
        (IpAddr::V4(ip), IpAddr::V4(net)) => ipv4_in_network(ip, net, prefix_len),
        (IpAddr::V6(ip), IpAddr::V6(net)) => ipv6_in_network(ip, net, prefix_len),
        _ => false,
    }
}

fn parse_cidr(cidr: &str) -> Option<(IpAddr, u8)> {
    let (addr_part, prefix_part) = cidr.split_once('/')?;
    let addr = addr_part.parse().ok()?;
    let prefix = prefix_part.parse().ok()?;
    Some((addr, prefix))
}

fn ipv4_in_network(ip: std::net::Ipv4Addr, network: std::net::Ipv4Addr, prefix_len: u8) -> bool {
    if prefix_len > 32 {
        return false;
    }
    let mask = if prefix_len == 0 {
        0
    } else {
        u32::MAX << (32 - prefix_len)
    };
    let ip_bits = u32::from_be_bytes(ip.octets());
    let net_bits = u32::from_be_bytes(network.octets());
    (ip_bits & mask) == (net_bits & mask)
}

fn ipv6_in_network(ip: std::net::Ipv6Addr, network: std::net::Ipv6Addr, prefix_len: u8) -> bool {
    if prefix_len > 128 {
        return false;
    }
    let ip_segments = ip.segments();
    let net_segments = network.segments();
    let full = prefix_len / 16;
    let rem = prefix_len % 16;

    for i in 0..full as usize {
        if ip_segments[i] != net_segments[i] {
            return false;
        }
    }

    if rem == 0 {
        return true;
    }

    let mask = u16::MAX << (16 - rem);
    (ip_segments[full as usize] & mask) == (net_segments[full as usize] & mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn publish_req(user: &str, pass: &str, path: &str, ip: &str) -> AuthRequest {
        AuthRequest {
            user: user.to_owned(),
            pass: pass.to_owned(),
            token: String::new(),
            ip: ip.to_owned(),
            path: path.to_owned(),
            action: Action::Publish,
            protocol: "rtsp".to_owned(),
        }
    }

    #[test]
    fn empty_users_allow_publish_and_read() {
        let auth = AuthManager::new(AuthMethod::Internal, vec![]);

        auth.authenticate(&publish_req("", "", "cam", "10.0.0.1"))
            .unwrap();
        auth.authenticate(&AuthRequest {
            action: Action::Read,
            ..publish_req("", "", "cam", "10.0.0.1")
        })
        .unwrap();
    }

    #[test]
    fn empty_users_deny_api() {
        let auth = AuthManager::new(AuthMethod::Internal, vec![]);
        let err = auth
            .authenticate(&AuthRequest {
                action: Action::Api,
                ..publish_req("", "", "", "127.0.0.1")
            })
            .unwrap_err();
        assert_eq!(err, AuthError::Denied);
    }

    #[test]
    fn configured_user_allow_and_deny() {
        let auth = AuthManager::new(
            AuthMethod::Internal,
            vec![AuthInternalUser {
                user: "pub".to_owned(),
                pass: "secret".to_owned(),
                ips: vec!["127.0.0.1/32".to_owned()],
                permissions: vec![AuthInternalUserPermission {
                    action: Action::Publish,
                    path: "live".to_owned(),
                }],
            }],
        );

        auth.authenticate(&publish_req("pub", "secret", "live", "127.0.0.1"))
            .unwrap();

        assert_eq!(
            auth.authenticate(&publish_req("", "", "live", "127.0.0.1"))
                .unwrap_err(),
            AuthError::CredentialsRequired
        );
        assert_eq!(
            auth.authenticate(&publish_req("pub", "wrong", "live", "127.0.0.1"))
                .unwrap_err(),
            AuthError::Denied
        );
        assert_eq!(
            auth.authenticate(&publish_req("pub", "secret", "other", "127.0.0.1"))
                .unwrap_err(),
            AuthError::Denied
        );
        assert_eq!(
            auth.authenticate(&publish_req("pub", "secret", "live", "10.0.0.1"))
                .unwrap_err(),
            AuthError::Denied
        );
        assert_eq!(
            auth.authenticate(&AuthRequest {
                action: Action::Read,
                ..publish_req("pub", "secret", "live", "127.0.0.1")
            })
            .unwrap_err(),
            AuthError::Denied
        );
    }

    #[test]
    fn any_user_skips_credential_check() {
        let auth = AuthManager::new(
            AuthMethod::Internal,
            vec![AuthInternalUser {
                user: "any".to_owned(),
                pass: String::new(),
                ips: vec![],
                permissions: vec![AuthInternalUserPermission {
                    action: Action::Playback,
                    path: String::new(),
                }],
            }],
        );

        auth.authenticate(&AuthRequest {
            user: String::new(),
            pass: String::new(),
            token: String::new(),
            ip: "192.168.1.1".to_owned(),
            path: "rec".to_owned(),
            action: Action::Playback,
            protocol: "hls".to_owned(),
        })
        .unwrap();
    }

    #[test]
    fn regex_path_permission() {
        let auth = AuthManager::new(
            AuthMethod::Internal,
            vec![AuthInternalUser {
                user: "any".to_owned(),
                pass: String::new(),
                ips: vec![],
                permissions: vec![AuthInternalUserPermission {
                    action: Action::Read,
                    path: "~^cam[0-9]+$".to_owned(),
                }],
            }],
        );

        auth.authenticate(&AuthRequest {
            action: Action::Read,
            path: "cam1".to_owned(),
            ..publish_req("", "", "", "127.0.0.1")
        })
        .unwrap();

        assert_eq!(
            auth.authenticate(&AuthRequest {
                action: Action::Read,
                path: "notcam".to_owned(),
                ..publish_req("", "", "", "127.0.0.1")
            })
            .unwrap_err(),
            AuthError::Denied
        );
    }

    #[test]
    fn http_and_jwt_not_implemented() {
        let req = publish_req("u", "p", "x", "127.0.0.1");
        assert_eq!(
            AuthManager::new(AuthMethod::Http, vec![])
                .authenticate(&req)
                .unwrap_err(),
            AuthError::NotImplemented(AuthMethod::Http)
        );
        assert_eq!(
            AuthManager::new(AuthMethod::Jwt, vec![])
                .authenticate(&req)
                .unwrap_err(),
            AuthError::NotImplemented(AuthMethod::Jwt)
        );
        let jwt = JwtConfig {
            jwks_url: "https://example.test/jwks".into(),
            ..Default::default()
        };
        assert_eq!(
            AuthManager::new(AuthMethod::Jwt, vec![])
                .with_jwt(jwt)
                .authenticate(&req)
                .unwrap_err(),
            AuthError::CredentialsRequired
        );
        let mut with_token = req.clone();
        with_token.token = "dev-token".into();
        AuthManager::new(AuthMethod::Jwt, vec![])
            .with_jwt(JwtConfig {
                jwks_url: "https://example.test/jwks".into(),
                ..Default::default()
            })
            .authenticate(&with_token)
            .unwrap();
    }
}
