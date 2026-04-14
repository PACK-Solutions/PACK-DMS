use sqlx::PgPool;
use std::collections::HashSet;
use uuid::Uuid;

/// Permission levels for document access control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    Read,
    Write,
    Admin,
}

impl Permission {
    /// Parse a permission string into a `Permission` variant.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "read" => Some(Self::Read),
            "write" => Some(Self::Write),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }

    /// Returns the set of permissions implied by this permission level.
    /// Admin implies write and read; write implies read.
    pub fn implied(self) -> HashSet<Permission> {
        match self {
            Self::Admin => [Self::Admin, Self::Write, Self::Read].into(),
            Self::Write => [Self::Write, Self::Read].into(),
            Self::Read => [Self::Read].into(),
        }
    }
}

/// The effective permission set for a user on a document.
#[derive(Debug, Clone)]
pub struct EffectivePermissions {
    pub permissions: HashSet<Permission>,
}

impl EffectivePermissions {
    /// Check whether the effective set includes the given permission.
    pub fn has(&self, perm: Permission) -> bool {
        self.permissions.contains(&perm)
    }
}

/// Service for resolving effective document-level permissions.
pub struct AclService;

impl AclService {
    /// Resolve the effective permissions for a user on a document.
    ///
    /// Rules:
    /// 1. The document owner always gets `admin` (which implies `write` and `read`).
    /// 2. Otherwise, collect all matching ACL entries for the user ID and their roles,
    ///    then compute the union of implied permissions.
    /// 3. If no explicit ACL exists for the document, walk up the parent hierarchy
    ///    to find inherited permissions.
    pub async fn effective_permissions(
        pool: &PgPool,
        user_id: Uuid,
        user_roles: &[String],
        document_id: Uuid,
    ) -> Result<EffectivePermissions, sqlx::Error> {
        // Check ownership first.
        let owner_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT owner_id FROM documents WHERE id = $1",
        )
        .bind(document_id)
        .fetch_optional(pool)
        .await?;

        if let Some(oid) = owner_id {
            if oid == user_id {
                return Ok(EffectivePermissions {
                    permissions: Permission::Admin.implied(),
                });
            }
        }

        // Try to resolve from explicit ACL entries, walking up the hierarchy.
        Self::resolve_with_inheritance(pool, user_id, user_roles, document_id).await
    }

    /// Resolve permissions for a document, walking up the parent chain if no
    /// explicit ACL entries exist for the current document.
    async fn resolve_with_inheritance(
        pool: &PgPool,
        user_id: Uuid,
        user_roles: &[String],
        document_id: Uuid,
    ) -> Result<EffectivePermissions, sqlx::Error> {
        // Limit hierarchy depth to prevent infinite loops.
        const MAX_DEPTH: usize = 20;
        let mut current_id = Some(document_id);
        let mut depth = 0;

        while let Some(doc_id) = current_id {
            if depth >= MAX_DEPTH {
                break;
            }
            depth += 1;

            let perms = Self::direct_permissions(pool, user_id, user_roles, doc_id).await?;
            if !perms.permissions.is_empty() {
                return Ok(perms);
            }

            // No explicit ACL — check parent.
            current_id = sqlx::query_scalar::<_, Uuid>(
                "SELECT parent_id FROM documents WHERE id = $1 AND parent_id IS NOT NULL",
            )
            .bind(doc_id)
            .fetch_optional(pool)
            .await?;
        }

        Ok(EffectivePermissions {
            permissions: HashSet::new(),
        })
    }

    /// Compute direct (non-inherited) permissions from ACL entries on a single document.
    async fn direct_permissions(
        pool: &PgPool,
        user_id: Uuid,
        user_roles: &[String],
        document_id: Uuid,
    ) -> Result<EffectivePermissions, sqlx::Error> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT permission FROM document_acl \
             WHERE document_id = $1 \
               AND ((principal_type = 'user' AND principal_id = $2) \
                 OR (principal_type = 'role' AND role = ANY($3)))",
        )
        .bind(document_id)
        .bind(user_id)
        .bind(user_roles)
        .fetch_all(pool)
        .await?;

        let mut permissions = HashSet::new();
        for (perm_str,) in &rows {
            if let Some(p) = Permission::from_str(perm_str) {
                permissions.extend(p.implied());
            }
        }

        Ok(EffectivePermissions { permissions })
    }

    /// Filter a list of document IDs to only those the user has at least `read` on.
    pub async fn filter_readable(
        pool: &PgPool,
        user_id: Uuid,
        user_roles: &[String],
        document_ids: &[Uuid],
    ) -> Result<HashSet<Uuid>, sqlx::Error> {
        if document_ids.is_empty() {
            return Ok(HashSet::new());
        }

        // Owner documents are always readable.
        let owned: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM documents WHERE id = ANY($1) AND owner_id = $2",
        )
        .bind(document_ids)
        .bind(user_id)
        .fetch_all(pool)
        .await?;

        let mut readable: HashSet<Uuid> = owned.into_iter().map(|(id,)| id).collect();

        // ACL-granted documents.
        let acl_granted: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT DISTINCT document_id FROM document_acl \
             WHERE document_id = ANY($1) \
               AND ((principal_type = 'user' AND principal_id = $2) \
                 OR (principal_type = 'role' AND role = ANY($3)))",
        )
        .bind(document_ids)
        .bind(user_id)
        .bind(user_roles)
        .fetch_all(pool)
        .await?;

        readable.extend(acl_granted.into_iter().map(|(id,)| id));

        // For documents not yet readable, check parent inheritance.
        for &doc_id in document_ids {
            if readable.contains(&doc_id) {
                continue;
            }
            let perms =
                Self::resolve_with_inheritance(pool, user_id, user_roles, doc_id).await?;
            if perms.has(Permission::Read) {
                readable.insert(doc_id);
            }
        }

        Ok(readable)
    }
}
