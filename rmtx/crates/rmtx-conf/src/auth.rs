//! Internal authentication user entries (`authInternalUsers`).
//!
//! Go counterpart: `internal/conf/auth_internal_user.go`.

use serde::{Deserialize, Serialize};

use crate::de;

/// Permission entry from `authInternalUsers[].permissions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AuthInternalUserPermission {
    pub action: String,
    pub path: String,
}

impl Default for AuthInternalUserPermission {
    fn default() -> Self {
        Self {
            action: String::new(),
            path: String::new(),
        }
    }
}

/// Internal user entry from config (`authInternalUsers`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AuthInternalUser {
    pub user: String,
    #[serde(deserialize_with = "de::null_string")]
    pub pass: String,
    pub ips: Vec<String>,
    pub permissions: Vec<AuthInternalUserPermission>,
}

impl Default for AuthInternalUser {
    fn default() -> Self {
        Self {
            user: String::new(),
            pass: String::new(),
            ips: Vec::new(),
            permissions: Vec::new(),
        }
    }
}
