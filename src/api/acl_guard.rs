use crate::api::error::{ProblemDetails, forbidden, internal};
use crate::domain::acl_service::{AclService, Permission};
use crate::infra::auth::AuthContext;
use sqlx::PgPool;
use uuid::Uuid;

/// Enforce that the caller has the required document-level permission.
///
/// Looks up the user's roles from the database, then delegates to `AclService`
/// to compute the effective permission set. Returns a `403 Forbidden` error
/// if the required permission is missing.
pub async fn enforce_permission(
    pool: &PgPool,
    auth: &AuthContext,
    document_id: Uuid,
    required: Permission,
) -> Result<(), ProblemDetails> {
    // Fetch user roles from the database.
    let roles: Vec<String> = sqlx::query_scalar(
        "SELECT unnest(roles) FROM users WHERE id = $1",
    )
    .bind(auth.user_id)
    .fetch_all(pool)
    .await
    .map_err(internal)?;

    let effective = AclService::effective_permissions(pool, auth.user_id, &roles, document_id)
        .await
        .map_err(internal)?;

    if effective.has(required) {
        Ok(())
    } else {
        Err(forbidden(format!(
            "you do not have {:?} permission on this document",
            required
        )))
    }
}
