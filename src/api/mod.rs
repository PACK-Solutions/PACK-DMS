mod acl;
mod audit;
mod documents;
pub mod error;
pub mod types;
mod versions;

use crate::domain::models::*;
use crate::infra::auth::AppState;
use axum::{
    Router,
    routing::{delete, get, post},
};
use error::ProblemDetails;
use std::sync::Arc;
use utoipa::OpenApi;

pub use types::{
    CreateDocumentRequest, DocumentResponse, LegalHoldRequest, PatchDocumentRequest,
    RetentionRequest, SearchQuery, StatusChangeRequest, UploadVersionRequest, VersionResponse,
};

#[derive(OpenApi)]
#[openapi(
    paths(
        documents::create_document,
        documents::get_document,
        versions::upload_version,
        versions::list_versions,
        versions::download_version,
        versions::delete_version,
        documents::search_documents,
        documents::patch_document,
        documents::change_status,
        documents::restore_document,
        documents::set_legal_hold,
        documents::set_retention,
        acl::get_acl,
        acl::put_acl,
        audit::list_audit
    ),
    components(
        schemas(
            CreateDocumentRequest,
            DocumentResponse,
            UploadVersionRequest,
            VersionResponse,
            Document,
            DocumentVersion,
            DocumentAcl,
            AuditLog,
            DocumentStatus,
            Blob,
            LegalHoldRequest,
            RetentionRequest,
            PatchDocumentRequest,
            StatusChangeRequest,
            ProblemDetails
        )
    ),
    modifiers(&SecurityAddon, &DescriptionAddon),
    tags(
        (name = "Documents", description = "Document lifecycle management — create, retrieve, search, update metadata, transition status, soft-delete, restore, and configure legal hold and retention policies."),
        (name = "Versions", description = "Binary content versioning — upload new versions (multipart), list version history, download a specific version, and soft-delete versions."),
        (name = "ACL", description = "Access Control Lists — view and replace per-document permission entries that grant read, write, or admin access to specific users."),
        (name = "Audit", description = "Audit trail — query the immutable log of every action performed on documents, versions, and ACLs.")
    ),
    info(
        title = "PackDMS API",
        version = "0.2.0"
    )
)]
pub struct ApiDoc;

pub struct DescriptionAddon;
impl utoipa::Modify for DescriptionAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        openapi.info.description = Some(include_str!("api-description.md").to_string());
    }
}

pub struct SecurityAddon;
impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::{Http, HttpAuthScheme, SecurityScheme};
        use utoipa::openapi::{Components, security::SecurityRequirement};
        let scheme = SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer));
        let components = openapi.components.get_or_insert_with(Components::default);
        components.add_security_scheme("bearerAuth", scheme);
        openapi.security = Some(vec![SecurityRequirement::new::<_, Vec<String>, _>(
            "bearerAuth",
            Vec::<String>::new(),
        )]);
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/documents",
            post(documents::create_document).get(documents::search_documents),
        )
        .route(
            "/documents/{id}",
            get(documents::get_document).patch(documents::patch_document),
        )
        .route("/documents/{id}/status", post(documents::change_status))
        .route("/documents/{id}/restore", post(documents::restore_document))
        .route(
            "/documents/{id}/legal-hold",
            post(documents::set_legal_hold),
        )
        .route("/documents/{id}/retention", post(documents::set_retention))
        .route(
            "/documents/{id}/versions",
            post(versions::upload_version).get(versions::list_versions),
        )
        .route(
            "/documents/{id}/versions/{vid}/download",
            get(versions::download_version),
        )
        .route(
            "/documents/{id}/versions/{vid}",
            delete(versions::delete_version),
        )
        .route("/documents/{id}/acl", get(acl::get_acl).put(acl::put_acl))
        .route("/audit", get(audit::list_audit))
        .with_state(state)
}
