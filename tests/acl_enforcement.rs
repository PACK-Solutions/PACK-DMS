use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use chrono::Utc;
use jsonwebtoken::jwk::{CommonParameters, Jwk, JwkSet, KeyAlgorithm, RSAKeyParameters};
use packdms::{api, domain::models::DocumentAcl, infra};
use sqlx::PgPool;
use std::sync::Arc;
use tower::util::ServiceExt;
use uuid::Uuid;

/// Shared test harness: pool, router, RSA key material, issuer.
struct Harness {
    pool: PgPool,
    state: Arc<infra::auth::AppState>,
    private_pem: String,
    kid: &'static str,
    issuer: &'static str,
}

impl Harness {
    async fn new() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();
        let db = std::env::var("DATABASE_URL").expect("DATABASE_URL required for tests");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&db)
            .await?;
        sqlx::migrate!().run(&pool).await?;

        let rsa_key = openssl::rsa::Rsa::generate(2048)?;
        let private_pem = String::from_utf8(rsa_key.private_key_to_pem()?)?;

        let n = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            rsa_key.n().to_vec(),
        );
        let e = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            rsa_key.e().to_vec(),
        );

        let kid = "test-kid";
        let jwk = Jwk {
            common: CommonParameters {
                public_key_use: None,
                key_operations: None,
                key_algorithm: Some(KeyAlgorithm::RS256),
                key_id: Some(kid.to_string()),
                x509_url: None,
                x509_chain: None,
                x509_sha1_fingerprint: None,
                x509_sha256_fingerprint: None,
            },
            algorithm: jsonwebtoken::jwk::AlgorithmParameters::RSA(RSAKeyParameters {
                key_type: jsonwebtoken::jwk::RSAKeyType::RSA,
                n,
                e,
            }),
        };
        let jwks = JwkSet { keys: vec![jwk] };

        let storage =
            Arc::new(infra::storage::MemoryBlobStore::new()) as Arc<dyn infra::storage::BlobStore>;
        let issuer = "test-issuer";
        let state = Arc::new(infra::auth::AppState {
            pool: pool.clone(),
            auth: infra::auth::AuthConfig {
                issuer: issuer.to_string(),
                jwks_url: "http://localhost/jwks".to_string(),
            },
            storage,
            jwks: Arc::new(jwks),
        });

        Ok(Self {
            pool,
            state,
            private_pem,
            kid,
            issuer,
        })
    }

    fn router(&self) -> axum::Router {
        api::router(self.state.clone())
    }

    async fn seed_user(&self, roles: &[&str]) -> anyhow::Result<(Uuid, String, String)> {
        let user_id = Uuid::new_v4();
        let email = format!("t+{}@example.com", Uuid::new_v4());
        let role_vec: Vec<String> = roles.iter().map(|r| r.to_string()).collect();
        sqlx::query(
            "INSERT INTO users (id, email, roles, status, created_at) VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(user_id)
        .bind(&email)
        .bind(&role_vec)
        .bind("active")
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;

        let scope = "document:read document:write";
        let token = infra::auth::issue_test_jwt(
            &self.private_pem,
            self.kid,
            self.issuer,
            user_id,
            &email,
            scope,
            3600,
        )?;
        Ok((user_id, email, token))
    }

    async fn seed_document(&self, owner_id: Uuid) -> anyhow::Result<Uuid> {
        let now = Utc::now();
        let doc = packdms::domain::models::Document {
            id: Uuid::new_v4(),
            title: "ACL Test Doc".into(),
            status: "draft".into(),
            owner_id,
            current_version_id: None,
            legal_hold: false,
            retention_until: None,
            metadata: serde_json::json!({}),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            deleted_by: None,
            archived_at: None,
            parent_id: None,
        };
        let mut tx = self.pool.begin().await?;
        infra::db::DocumentRepo::create(&mut tx, &doc).await?;

        // Auto-create owner ACL entry (mirrors create_document handler).
        let owner_acl = DocumentAcl {
            id: Uuid::new_v4(),
            document_id: doc.id,
            principal_type: "user".to_string(),
            principal_id: Some(owner_id),
            role: None,
            permission: "admin".to_string(),
        };
        infra::db::AclRepo::create(&mut tx, &owner_acl).await?;

        tx.commit().await?;
        Ok(doc.id)
    }

    async fn add_acl(
        &self,
        document_id: Uuid,
        principal_type: &str,
        principal_id: Option<Uuid>,
        role: Option<&str>,
        permission: &str,
    ) -> anyhow::Result<()> {
        let acl = DocumentAcl {
            id: Uuid::new_v4(),
            document_id,
            principal_type: principal_type.to_string(),
            principal_id,
            role: role.map(|r| r.to_string()),
            permission: permission.to_string(),
        };
        let mut tx = self.pool.begin().await?;
        infra::db::AclRepo::create(&mut tx, &acl).await?;
        tx.commit().await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Test 1: User without any ACL entry receives 403 on GET /documents/{id}
// ---------------------------------------------------------------------------
#[tokio::test]
async fn user_without_acl_gets_403() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, _) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    // Create a second user with NO ACL on the document.
    let (_, _, other_token) = h.seed_user(&["user"]).await?;

    let req = Request::builder()
        .method("GET")
        .uri(format!("/documents/{doc_id}"))
        .header("authorization", format!("Bearer {other_token}"))
        .body(Body::empty())?;

    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    Ok(())
}

// ---------------------------------------------------------------------------
// Test 2: Document owner gets implicit admin (can GET the document)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn owner_gets_implicit_admin() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, owner_token) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    let req = Request::builder()
        .method("GET")
        .uri(format!("/documents/{doc_id}"))
        .header("authorization", format!("Bearer {owner_token}"))
        .body(Body::empty())?;

    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    Ok(())
}

// ---------------------------------------------------------------------------
// Test 3: Role-based ACL grant works
// ---------------------------------------------------------------------------
#[tokio::test]
async fn role_based_acl_grant_works() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, _) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    // Create a user with role "editors".
    let (_, _, editor_token) = h.seed_user(&["editors"]).await?;

    // Without ACL → 403
    let req = Request::builder()
        .method("GET")
        .uri(format!("/documents/{doc_id}"))
        .header("authorization", format!("Bearer {editor_token}"))
        .body(Body::empty())?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // Grant read to role "editors"
    h.add_acl(doc_id, "role", None, Some("editors"), "read")
        .await?;

    // Now should succeed
    let req = Request::builder()
        .method("GET")
        .uri(format!("/documents/{doc_id}"))
        .header("authorization", format!("Bearer {editor_token}"))
        .body(Body::empty())?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    Ok(())
}

// ---------------------------------------------------------------------------
// Test 4: Inherited permissions resolve correctly via parent_id
// ---------------------------------------------------------------------------
#[tokio::test]
async fn inherited_permissions_from_parent() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, _) = h.seed_user(&["user"]).await?;

    // Create parent (folder) document.
    let parent_id = h.seed_document(owner_id).await?;

    // Create child document with parent_id set, but NO explicit ACL.
    let now = Utc::now();
    let child = packdms::domain::models::Document {
        id: Uuid::new_v4(),
        title: "Child Doc".into(),
        status: "draft".into(),
        owner_id,
        current_version_id: None,
        legal_hold: false,
        retention_until: None,
        metadata: serde_json::json!({}),
        created_at: now,
        updated_at: now,
        deleted_at: None,
        deleted_by: None,
        archived_at: None,
        parent_id: Some(parent_id),
    };
    let mut tx = h.pool.begin().await?;
    infra::db::DocumentRepo::create(&mut tx, &child).await?;
    // Intentionally NO ACL entry for the child.
    tx.commit().await?;

    // Create a third user, grant them read on the PARENT.
    let (reader_id, _, reader_token) = h.seed_user(&["user"]).await?;
    h.add_acl(parent_id, "user", Some(reader_id), None, "read")
        .await?;

    // Reader should be able to GET the child via inherited permission.
    let req = Request::builder()
        .method("GET")
        .uri(format!("/documents/{}", child.id))
        .header("authorization", format!("Bearer {reader_token}"))
        .body(Body::empty())?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    Ok(())
}

// ---------------------------------------------------------------------------
// Test 5: search_documents filters out documents without read permission
// ---------------------------------------------------------------------------
#[tokio::test]
async fn search_filters_by_acl() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, _) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    // Second user has no ACL.
    let (_, _, other_token) = h.seed_user(&["user"]).await?;

    let req = Request::builder()
        .method("GET")
        .uri("/documents")
        .header("authorization", format!("Bearer {other_token}"))
        .body(Body::empty())?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 1_000_000).await?;
    let docs: Vec<serde_json::Value> = serde_json::from_slice(&body)?;
    // The document owned by someone else should NOT appear.
    assert!(
        !docs.iter().any(|d| d["id"] == doc_id.to_string()),
        "document should be filtered out for user without ACL"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Test 6: PATCH /documents/{id}/acl adds and removes entries
// ---------------------------------------------------------------------------
#[tokio::test]
async fn patch_acl_add_and_remove() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, owner_token) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    let (reader_id, _, reader_token) = h.seed_user(&["user"]).await?;

    // PATCH: add read for reader
    let patch_body = serde_json::json!([
        { "op": "add", "principal_type": "user", "principal_id": reader_id, "permission": "read" }
    ]);
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/documents/{doc_id}/acl"))
        .header("authorization", format!("Bearer {owner_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&patch_body)?))?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Reader can now GET the document.
    let req = Request::builder()
        .method("GET")
        .uri(format!("/documents/{doc_id}"))
        .header("authorization", format!("Bearer {reader_token}"))
        .body(Body::empty())?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    // PATCH: remove read for reader
    let patch_body = serde_json::json!([
        { "op": "remove", "principal_type": "user", "principal_id": reader_id, "permission": "read" }
    ]);
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/documents/{doc_id}/acl"))
        .header("authorization", format!("Bearer {owner_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&patch_body)?))?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Reader should now get 403.
    let req = Request::builder()
        .method("GET")
        .uri(format!("/documents/{doc_id}"))
        .header("authorization", format!("Bearer {reader_token}"))
        .body(Body::empty())?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    Ok(())
}
