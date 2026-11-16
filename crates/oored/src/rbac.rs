use axum::Json;
use axum::http::StatusCode;
use casbin::prelude::*;
use oore_contract::ApiError;
use tracing::warn;

use crate::session::{Ability, current_has_ability};
use crate::util::api_err;

/// The token ability an instance-level `resource`/`action` pair needs.
pub fn ability_for(resource: &str, action: &str) -> Ability {
    match (resource, action) {
        (_, "read") => Ability::Read,
        ("builds", "write" | "cancel") | ("artifacts", "write") => Ability::Build,
        _ => Ability::Manage,
    }
}

const MODEL: &str = include_str!("../rbac_model.conf");
const POLICY: &str = include_str!("../rbac_policy.csv");

/// A thin wrapper around a Casbin `Enforcer` for RBAC policy checks.
pub struct CasbinEnforcer {
    enforcer: Enforcer,
}

impl CasbinEnforcer {
    /// Check whether `role` is allowed to perform `action` on `resource`.
    ///
    /// Inside a request signed in with an API token, the token must also hold
    /// the ability the pair needs (see `ability_for`).
    pub async fn check(&self, role: &str, resource: &str, action: &str) -> bool {
        if !current_has_ability(ability_for(resource, action)) {
            return false;
        }
        match self.enforcer.enforce((role, resource, action)) {
            Ok(allowed) => allowed,
            Err(e) => {
                warn!(error = %e, role, resource, action, "casbin enforce error — denying access");
                false
            }
        }
    }
}

/// Initialise a Casbin enforcer with the embedded RBAC model and policy.
pub async fn init_enforcer() -> anyhow::Result<CasbinEnforcer> {
    let model = DefaultModel::from_str(MODEL).await?;
    let adapter = StringAdapter::new(POLICY.to_string());
    let enforcer = Enforcer::new(model, adapter).await?;
    Ok(CasbinEnforcer { enforcer })
}

/// Check whether `role` has permission to perform `action` on `resource`.
///
/// Returns `Ok(())` if permitted, or an HTTP 403 error if denied.
pub async fn check_permission(
    enforcer: &CasbinEnforcer,
    role: &str,
    resource: &str,
    action: &str,
) -> std::result::Result<(), (StatusCode, Json<ApiError>)> {
    if enforcer.check(role, resource, action).await {
        Ok(())
    } else {
        Err(api_err(
            StatusCode::FORBIDDEN,
            "permission_denied",
            "You do not have permission to perform this action",
        ))
    }
}
