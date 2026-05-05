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

    /// Create a child document with the given parent and no ACL entries.
    async fn seed_child_document(&self, owner_id: Uuid, parent_id: Uuid) -> anyhow::Result<Uuid> {
        let now = Utc::now();
        let child = packdms::domain::models::Document {
            id: Uuid::new_v4(),
            title: "Child".into(),
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
        let mut tx = self.pool.begin().await?;
        infra::db::DocumentRepo::create(&mut tx, &child).await?;
        tx.commit().await?;
        Ok(child.id)
    }

    /// Build a multipart body for an upload request.
    fn multipart_body(
        boundary: &str,
        name: &str,
        filename: &str,
        content_type: &str,
        content: &str,
    ) -> (String, Vec<u8>) {
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
        body.extend_from_slice(content.as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        (format!("multipart/form-data; boundary={boundary}"), body)
    }

    /// Upload a version as the given user. Returns the version id.
    async fn upload_version_as(
        &self,
        doc_id: Uuid,
        token: &str,
        content: &str,
    ) -> anyhow::Result<Uuid> {
        let boundary = format!("boundary-{}", Uuid::new_v4());
        let (ct, body) = Self::multipart_body(&boundary, "file", "f.txt", "text/plain", content);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/documents/{doc_id}/versions"))
            .header("content-type", ct)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(body))?;
        let resp = self.router().oneshot(req).await?;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(resp.into_body(), 1_000_000).await?;
        let v: serde_json::Value = serde_json::from_slice(&bytes)?;
        Ok(Uuid::parse_str(v["id"].as_str().unwrap())?)
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

// ---------------------------------------------------------------------------
// Test 7: Permission hierarchy — write grant allows PATCH document
// ---------------------------------------------------------------------------
#[tokio::test]
async fn write_grant_allows_patch_document() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, _) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    let (writer_id, _, writer_token) = h.seed_user(&["user"]).await?;
    h.add_acl(doc_id, "user", Some(writer_id), None, "write")
        .await?;

    let body = serde_json::json!({ "title": "Updated by writer" });
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/documents/{doc_id}"))
        .header("authorization", format!("Bearer {writer_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body)?))?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    Ok(())
}

// ---------------------------------------------------------------------------
// Test 8: Permission hierarchy — read grant blocks PATCH document
// ---------------------------------------------------------------------------
#[tokio::test]
async fn read_grant_blocks_patch_document() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, _) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    let (reader_id, _, reader_token) = h.seed_user(&["user"]).await?;
    h.add_acl(doc_id, "user", Some(reader_id), None, "read")
        .await?;

    let body = serde_json::json!({ "title": "Should be denied" });
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/documents/{doc_id}"))
        .header("authorization", format!("Bearer {reader_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body)?))?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    Ok(())
}

// ---------------------------------------------------------------------------
// Test 9: Permission hierarchy — admin grant allows PATCH ACL
// ---------------------------------------------------------------------------
#[tokio::test]
async fn admin_grant_allows_patch_acl() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, _) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    let (admin_user_id, _, admin_token) = h.seed_user(&["user"]).await?;
    h.add_acl(doc_id, "user", Some(admin_user_id), None, "admin")
        .await?;

    // Admin user adds a third user with read.
    let (third_id, _, _) = h.seed_user(&["user"]).await?;
    let body = serde_json::json!([
        { "op": "add", "principal_type": "user", "principal_id": third_id, "permission": "read" }
    ]);
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/documents/{doc_id}/acl"))
        .header("authorization", format!("Bearer {admin_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body)?))?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    Ok(())
}

// ---------------------------------------------------------------------------
// Test 10: Permission hierarchy — write grant blocks PATCH ACL (admin needed)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn write_grant_blocks_patch_acl() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, _) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    let (writer_id, _, writer_token) = h.seed_user(&["user"]).await?;
    h.add_acl(doc_id, "user", Some(writer_id), None, "write")
        .await?;

    let (third_id, _, _) = h.seed_user(&["user"]).await?;
    let body = serde_json::json!([
        { "op": "add", "principal_type": "user", "principal_id": third_id, "permission": "read" }
    ]);
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/documents/{doc_id}/acl"))
        .header("authorization", format!("Bearer {writer_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body)?))?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    Ok(())
}

// ---------------------------------------------------------------------------
// Test 11: PUT /documents/{id}/acl replaces all entries
// ---------------------------------------------------------------------------
#[tokio::test]
async fn put_acl_replaces_all_entries() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, owner_token) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    // Pre-seed two entries that should be wiped by PUT.
    let (alice_id, _, alice_token) = h.seed_user(&["user"]).await?;
    let (bob_id, _, bob_token) = h.seed_user(&["user"]).await?;
    h.add_acl(doc_id, "user", Some(alice_id), None, "read")
        .await?;
    h.add_acl(doc_id, "user", Some(bob_id), None, "write")
        .await?;

    // Sanity: alice can GET before PUT.
    let req = Request::builder()
        .method("GET")
        .uri(format!("/documents/{doc_id}"))
        .header("authorization", format!("Bearer {alice_token}"))
        .body(Body::empty())?;
    assert_eq!(h.router().oneshot(req).await?.status(), StatusCode::OK);

    // Owner replaces ACL with only Bob (write).
    let new_rules = serde_json::json!([
        {
            "id": Uuid::new_v4(),
            "document_id": doc_id,
            "principal_type": "user",
            "principal_id": bob_id,
            "role": null,
            "permission": "write"
        }
    ]);
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/documents/{doc_id}/acl"))
        .header("authorization", format!("Bearer {owner_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&new_rules)?))?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Alice now denied; Bob still allowed.
    let req = Request::builder()
        .method("GET")
        .uri(format!("/documents/{doc_id}"))
        .header("authorization", format!("Bearer {alice_token}"))
        .body(Body::empty())?;
    assert_eq!(
        h.router().oneshot(req).await?.status(),
        StatusCode::FORBIDDEN
    );

    let req = Request::builder()
        .method("GET")
        .uri(format!("/documents/{doc_id}"))
        .header("authorization", format!("Bearer {bob_token}"))
        .body(Body::empty())?;
    assert_eq!(h.router().oneshot(req).await?.status(), StatusCode::OK);

    Ok(())
}

// ---------------------------------------------------------------------------
// Test 12: PUT /documents/{id}/acl requires admin permission (write only is 403)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn put_acl_requires_admin() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, _) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    let (writer_id, _, writer_token) = h.seed_user(&["user"]).await?;
    h.add_acl(doc_id, "user", Some(writer_id), None, "write")
        .await?;

    let new_rules: serde_json::Value = serde_json::json!([]);
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/documents/{doc_id}/acl"))
        .header("authorization", format!("Bearer {writer_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&new_rules)?))?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    Ok(())
}

// ---------------------------------------------------------------------------
// Test 13: Owner retains implicit admin even if PUT ACL removes their entry
// ---------------------------------------------------------------------------
#[tokio::test]
async fn owner_keeps_implicit_admin_after_put_removes_entry() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, owner_token) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    // Owner replaces ACL with empty set — wipes owner's auto-created admin entry.
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/documents/{doc_id}/acl"))
        .header("authorization", format!("Bearer {owner_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&serde_json::json!([]))?))?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Owner can still GET (implicit admin via owner_id).
    let req = Request::builder()
        .method("GET")
        .uri(format!("/documents/{doc_id}"))
        .header("authorization", format!("Bearer {owner_token}"))
        .body(Body::empty())?;
    assert_eq!(h.router().oneshot(req).await?.status(), StatusCode::OK);

    // Owner can still PATCH ACL (implicit admin allows the admin-required action).
    let (third_id, _, _) = h.seed_user(&["user"]).await?;
    let body = serde_json::json!([
        { "op": "add", "principal_type": "user", "principal_id": third_id, "permission": "read" }
    ]);
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/documents/{doc_id}/acl"))
        .header("authorization", format!("Bearer {owner_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body)?))?;
    assert_eq!(
        h.router().oneshot(req).await?.status(),
        StatusCode::NO_CONTENT
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Test 14: Versions endpoints enforce ACL (list/upload/download/delete)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn versions_endpoints_enforce_acl() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, owner_token) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    // Owner uploads an initial version.
    let version_id = h
        .upload_version_as(doc_id, &owner_token, "owner-blob")
        .await?;

    // Outsider has no ACL on the document.
    let (_, _, outsider_token) = h.seed_user(&["user"]).await?;

    // GET list versions → 403
    let req = Request::builder()
        .method("GET")
        .uri(format!("/documents/{doc_id}/versions"))
        .header("authorization", format!("Bearer {outsider_token}"))
        .body(Body::empty())?;
    assert_eq!(
        h.router().oneshot(req).await?.status(),
        StatusCode::FORBIDDEN
    );

    // GET download → 403
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/documents/{doc_id}/versions/{version_id}/download"
        ))
        .header("authorization", format!("Bearer {outsider_token}"))
        .body(Body::empty())?;
    assert_eq!(
        h.router().oneshot(req).await?.status(),
        StatusCode::FORBIDDEN
    );

    // POST upload → 403
    let boundary = format!("boundary-{}", Uuid::new_v4());
    let (ct, body) =
        Harness::multipart_body(&boundary, "file", "x.txt", "text/plain", "outsider-payload");
    let req = Request::builder()
        .method("POST")
        .uri(format!("/documents/{doc_id}/versions"))
        .header("content-type", ct)
        .header("authorization", format!("Bearer {outsider_token}"))
        .body(Body::from(body))?;
    assert_eq!(
        h.router().oneshot(req).await?.status(),
        StatusCode::FORBIDDEN
    );

    // DELETE version → 403
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/documents/{doc_id}/versions/{version_id}"))
        .header("authorization", format!("Bearer {outsider_token}"))
        .body(Body::empty())?;
    assert_eq!(
        h.router().oneshot(req).await?.status(),
        StatusCode::FORBIDDEN
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Test 15: Versions endpoints — read permission allows list+download but not upload+delete
// ---------------------------------------------------------------------------
#[tokio::test]
async fn read_permission_allows_versions_read_only() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, owner_token) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;
    let version_id = h.upload_version_as(doc_id, &owner_token, "v1").await?;

    let (reader_id, _, reader_token) = h.seed_user(&["user"]).await?;
    h.add_acl(doc_id, "user", Some(reader_id), None, "read")
        .await?;

    // List versions → 200
    let req = Request::builder()
        .method("GET")
        .uri(format!("/documents/{doc_id}/versions"))
        .header("authorization", format!("Bearer {reader_token}"))
        .body(Body::empty())?;
    assert_eq!(h.router().oneshot(req).await?.status(), StatusCode::OK);

    // Download → 200
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/documents/{doc_id}/versions/{version_id}/download"
        ))
        .header("authorization", format!("Bearer {reader_token}"))
        .body(Body::empty())?;
    assert_eq!(h.router().oneshot(req).await?.status(), StatusCode::OK);

    // Upload → 403 (write needed)
    let boundary = format!("boundary-{}", Uuid::new_v4());
    let (ct, body) =
        Harness::multipart_body(&boundary, "file", "y.txt", "text/plain", "should-fail");
    let req = Request::builder()
        .method("POST")
        .uri(format!("/documents/{doc_id}/versions"))
        .header("content-type", ct)
        .header("authorization", format!("Bearer {reader_token}"))
        .body(Body::from(body))?;
    assert_eq!(
        h.router().oneshot(req).await?.status(),
        StatusCode::FORBIDDEN
    );

    // Delete version → 403 (write needed)
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/documents/{doc_id}/versions/{version_id}"))
        .header("authorization", format!("Bearer {reader_token}"))
        .body(Body::empty())?;
    assert_eq!(
        h.router().oneshot(req).await?.status(),
        StatusCode::FORBIDDEN
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Test 16: GET /documents/{id}/acl requires read permission on the document
// ---------------------------------------------------------------------------
#[tokio::test]
async fn get_acl_requires_read_permission() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, owner_token) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    // Outsider with no ACL → 403
    let (_, _, outsider_token) = h.seed_user(&["user"]).await?;
    let req = Request::builder()
        .method("GET")
        .uri(format!("/documents/{doc_id}/acl"))
        .header("authorization", format!("Bearer {outsider_token}"))
        .body(Body::empty())?;
    assert_eq!(
        h.router().oneshot(req).await?.status(),
        StatusCode::FORBIDDEN
    );

    // Owner can GET ACL — at minimum the auto-created admin entry.
    let req = Request::builder()
        .method("GET")
        .uri(format!("/documents/{doc_id}/acl"))
        .header("authorization", format!("Bearer {owner_token}"))
        .body(Body::empty())?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1_000_000).await?;
    let entries: Vec<serde_json::Value> = serde_json::from_slice(&bytes)?;
    assert!(!entries.is_empty(), "owner ACL entry should be present");

    Ok(())
}

// ---------------------------------------------------------------------------
// Test 17: PATCH ACL with unknown op returns 400
// ---------------------------------------------------------------------------
#[tokio::test]
async fn patch_acl_unknown_op_returns_400() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, owner_token) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    let (target_id, _, _) = h.seed_user(&["user"]).await?;
    let body = serde_json::json!([
        { "op": "replace", "principal_type": "user", "principal_id": target_id, "permission": "read" }
    ]);
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/documents/{doc_id}/acl"))
        .header("authorization", format!("Bearer {owner_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body)?))?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    Ok(())
}

// ---------------------------------------------------------------------------
// Test 18: PATCH ACL rejects invalid permission strings
// ---------------------------------------------------------------------------
#[tokio::test]
async fn patch_acl_invalid_permission_returns_400() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, owner_token) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    let (target_id, _, _) = h.seed_user(&["user"]).await?;
    let body = serde_json::json!([
        { "op": "add", "principal_type": "user", "principal_id": target_id, "permission": "delete" }
    ]);
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/documents/{doc_id}/acl"))
        .header("authorization", format!("Bearer {owner_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body)?))?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    Ok(())
}

// ---------------------------------------------------------------------------
// Test 19: PATCH ACL rejects inconsistent (principal_type, principal_id, role)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn patch_acl_user_without_principal_id_returns_400() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, owner_token) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    // principal_type=user but no principal_id
    let body = serde_json::json!([
        { "op": "add", "principal_type": "user", "permission": "read" }
    ]);
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/documents/{doc_id}/acl"))
        .header("authorization", format!("Bearer {owner_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body)?))?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    Ok(())
}

#[tokio::test]
async fn patch_acl_role_without_role_field_returns_400() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, owner_token) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    // principal_type=role but no role
    let body = serde_json::json!([
        { "op": "add", "principal_type": "role", "permission": "read" }
    ]);
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/documents/{doc_id}/acl"))
        .header("authorization", format!("Bearer {owner_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body)?))?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    Ok(())
}

#[tokio::test]
async fn patch_acl_unknown_principal_type_returns_400() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, owner_token) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    let body = serde_json::json!([
        { "op": "add", "principal_type": "everyone", "permission": "read" }
    ]);
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/documents/{doc_id}/acl"))
        .header("authorization", format!("Bearer {owner_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body)?))?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    Ok(())
}

// ---------------------------------------------------------------------------
// Test 20: PATCH ACL "add" of an existing principal/permission is idempotent
// ---------------------------------------------------------------------------
#[tokio::test]
async fn patch_acl_add_is_idempotent_for_same_entry() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, owner_token) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    let (reader_id, _, _) = h.seed_user(&["user"]).await?;

    // Add read for reader, twice.
    for _ in 0..2 {
        let body = serde_json::json!([
            { "op": "add", "principal_type": "user", "principal_id": reader_id, "permission": "read" }
        ]);
        let req = Request::builder()
            .method("PATCH")
            .uri(format!("/documents/{doc_id}/acl"))
            .header("authorization", format!("Bearer {owner_token}"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body)?))?;
        let resp = h.router().oneshot(req).await?;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    // Verify only ONE entry exists for that principal/permission.
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM document_acl \
         WHERE document_id = $1 AND principal_type = 'user' \
           AND principal_id = $2 AND permission = 'read'",
    )
    .bind(doc_id)
    .bind(reader_id)
    .fetch_one(&h.pool)
    .await?;
    assert_eq!(
        count.0, 1,
        "duplicate add must not create duplicate ACL rows"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Test 21: PATCH ACL "add" upgrade replaces previous permission for same principal
// ---------------------------------------------------------------------------
#[tokio::test]
async fn patch_acl_add_upgrade_replaces_previous() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, owner_token) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    let (target_id, _, target_token) = h.seed_user(&["user"]).await?;

    // First: grant read.
    let body = serde_json::json!([
        { "op": "add", "principal_type": "user", "principal_id": target_id, "permission": "read" }
    ]);
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/documents/{doc_id}/acl"))
        .header("authorization", format!("Bearer {owner_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body)?))?;
    assert_eq!(
        h.router().oneshot(req).await?.status(),
        StatusCode::NO_CONTENT
    );

    // Then upgrade to write.
    let body = serde_json::json!([
        { "op": "add", "principal_type": "user", "principal_id": target_id, "permission": "write" }
    ]);
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/documents/{doc_id}/acl"))
        .header("authorization", format!("Bearer {owner_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body)?))?;
    assert_eq!(
        h.router().oneshot(req).await?.status(),
        StatusCode::NO_CONTENT
    );

    // Exactly one ACL row for the user; permission should be 'write'.
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT permission FROM document_acl \
         WHERE document_id = $1 AND principal_type = 'user' AND principal_id = $2",
    )
    .bind(doc_id)
    .bind(target_id)
    .fetch_all(&h.pool)
    .await?;
    assert_eq!(rows.len(), 1, "only one ACL row should remain");
    assert_eq!(rows[0].0, "write", "permission must be the upgraded one");

    // Functional check: target can now PATCH document (write needed).
    let body = serde_json::json!({ "title": "Edited by upgraded user" });
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/documents/{doc_id}"))
        .header("authorization", format!("Bearer {target_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body)?))?;
    assert_eq!(h.router().oneshot(req).await?.status(), StatusCode::OK);

    Ok(())
}

// ---------------------------------------------------------------------------
// Test 22: PUT ACL rejects invalid permission strings
// ---------------------------------------------------------------------------
#[tokio::test]
async fn put_acl_invalid_permission_returns_400() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, owner_token) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    let (target_id, _, _) = h.seed_user(&["user"]).await?;
    let rules = serde_json::json!([
        {
            "id": Uuid::new_v4(),
            "document_id": doc_id,
            "principal_type": "user",
            "principal_id": target_id,
            "role": null,
            "permission": "execute"
        }
    ]);
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/documents/{doc_id}/acl"))
        .header("authorization", format!("Bearer {owner_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&rules)?))?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    Ok(())
}

// ---------------------------------------------------------------------------
// Test 23: Multi-level inheritance — grandchild inherits via grandparent
// ---------------------------------------------------------------------------
#[tokio::test]
async fn multi_level_parent_inheritance() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, _) = h.seed_user(&["user"]).await?;
    let grandparent_id = h.seed_document(owner_id).await?;
    let parent_id = h.seed_child_document(owner_id, grandparent_id).await?;
    let child_id = h.seed_child_document(owner_id, parent_id).await?;

    // Grant a third user read on the GRANDPARENT.
    let (reader_id, _, reader_token) = h.seed_user(&["user"]).await?;
    h.add_acl(grandparent_id, "user", Some(reader_id), None, "read")
        .await?;

    // Reader should be able to GET the grandchild via two-level inheritance.
    let req = Request::builder()
        .method("GET")
        .uri(format!("/documents/{child_id}"))
        .header("authorization", format!("Bearer {reader_token}"))
        .body(Body::empty())?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    Ok(())
}

// ---------------------------------------------------------------------------
// Test 24: Role-based inheritance — child inherits a role grant from the parent
// ---------------------------------------------------------------------------
#[tokio::test]
async fn role_based_inheritance_from_parent() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, _) = h.seed_user(&["user"]).await?;
    let parent_id = h.seed_document(owner_id).await?;
    let child_id = h.seed_child_document(owner_id, parent_id).await?;

    h.add_acl(parent_id, "role", None, Some("viewers"), "read")
        .await?;

    let (_, _, viewer_token) = h.seed_user(&["viewers"]).await?;
    let req = Request::builder()
        .method("GET")
        .uri(format!("/documents/{child_id}"))
        .header("authorization", format!("Bearer {viewer_token}"))
        .body(Body::empty())?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    Ok(())
}

// ---------------------------------------------------------------------------
// Test 25: User and role grants on the same document are unioned
// ---------------------------------------------------------------------------
#[tokio::test]
async fn user_and_role_grants_are_unioned() -> anyhow::Result<()> {
    let h = Harness::new().await?;
    let (owner_id, _, _) = h.seed_user(&["user"]).await?;
    let doc_id = h.seed_document(owner_id).await?;

    // Target user has role "editors". Grant user_id read; grant role "editors" write.
    let (target_id, _, target_token) = h.seed_user(&["editors"]).await?;
    h.add_acl(doc_id, "user", Some(target_id), None, "read")
        .await?;
    h.add_acl(doc_id, "role", None, Some("editors"), "write")
        .await?;

    // Target should be able to PATCH document (write was granted via the role).
    let body = serde_json::json!({ "title": "Updated via role grant" });
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/documents/{doc_id}"))
        .header("authorization", format!("Bearer {target_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body)?))?;
    let resp = h.router().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    // But not PATCH the ACL (admin not granted).
    let body = serde_json::json!([
        { "op": "add", "principal_type": "user", "principal_id": owner_id, "permission": "read" }
    ]);
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/documents/{doc_id}/acl"))
        .header("authorization", format!("Bearer {target_token}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body)?))?;
    assert_eq!(
        h.router().oneshot(req).await?.status(),
        StatusCode::FORBIDDEN
    );

    Ok(())
}
