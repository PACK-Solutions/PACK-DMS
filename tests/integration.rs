use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::Response,
};
use chrono::Utc;
use jsonwebtoken::jwk::{CommonParameters, Jwk, JwkSet, KeyAlgorithm, RSAKeyParameters};
use packdms::{api, infra};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tower::util::ServiceExt; // for `oneshot`
use uuid::Uuid;

#[tokio::test]
async fn upload_version_updates_current_and_increments() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let db = std::env::var("DATABASE_URL").expect("DATABASE_URL required for tests");
    let pool = PgPoolOptions::new().max_connections(2).connect(&db).await?;
    sqlx::migrate!().run(&pool).await?;

    // Setup RSA keys for testing
    let rsa_key = openssl::rsa::Rsa::generate(2048)?;
    let private_pem = String::from_utf8(rsa_key.private_key_to_pem()?)?;
    // public_pem is not needed if we build JWK directly from rsa_key

    // Create a JWK from the public key
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

    // create state with memory storage
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

    // seed a user with a unique email to avoid conflicts on repeated runs
    let user_id = Uuid::new_v4();
    let email = format!("t+{}@example.com", Uuid::new_v4());
    sqlx::query("INSERT INTO users (id, email, roles, status, created_at) VALUES ($1,$2,$3,$4,$5)")
        .bind(user_id)
        .bind(&email)
        .bind(vec!["user".to_string()])
        .bind("active")
        .bind(Utc::now())
        .execute(&pool)
        .await?;

    // create document via repo
    let mut tx = pool.begin().await?;
    let now = Utc::now();
    let doc = packdms::domain::models::Document {
        id: Uuid::new_v4(),
        title: "Test".into(),
        status: "draft".into(),
        owner_id: user_id,
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
    infra::db::DocumentRepo::create(&mut tx, &doc).await?;
    // Auto-create owner ACL entry (mirrors create_document handler).
    let owner_acl = packdms::domain::models::DocumentAcl {
        id: Uuid::new_v4(),
        document_id: doc.id,
        principal_type: "user".to_string(),
        principal_id: Some(user_id),
        role: None,
        permission: "admin".to_string(),
    };
    infra::db::AclRepo::create(&mut tx, &owner_acl).await?;
    tx.commit().await?;

    // build router
    let router = api::router(state.clone());

    // issue JWT for requests
    let token = infra::auth::issue_test_jwt(
        &private_pem,
        kid,
        issuer,
        user_id,
        &email,
        "document:read document:write",
        3600,
    )?;

    // helper to build multipart body
    fn multipart_body(
        boundary: &str,
        name: &str,
        filename: &str,
        content_type: &str,
        content: &str,
    ) -> (String, Vec<u8>) {
        let mut body = Vec::new();
        let mut push = |s: &str| body.extend_from_slice(s.as_bytes());
        push(&format!("--{}\r\n", boundary));
        push(&format!(
            "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
            name, filename
        ));
        push(&format!("Content-Type: {}\r\n\r\n", content_type));
        push(content);
        push("\r\n");
        push(&format!("--{}--\r\n", boundary));
        (format!("multipart/form-data; boundary={}", boundary), body)
    }

    // first upload
    let boundary1 = format!("boundary-{}", Uuid::new_v4());
    let (ct1, body1) = multipart_body(&boundary1, "file", "hello.txt", "text/plain", "hello");
    let req1 = Request::builder()
        .method("POST")
        .uri(format!("/documents/{}/versions", doc.id))
        .header("content-type", ct1)
        .header("authorization", format!("Bearer {}", token))
        .body(Body::from(body1))?;
    let resp1: Response = router.clone().oneshot(req1).await?;
    assert_eq!(resp1.status(), StatusCode::CREATED);

    // second upload
    let boundary2 = format!("boundary-{}", Uuid::new_v4());
    let (ct2, body2) = multipart_body(&boundary2, "file", "world.txt", "text/plain", "world");
    let req2 = Request::builder()
        .method("POST")
        .uri(format!("/documents/{}/versions", doc.id))
        .header("content-type", ct2)
        .header("authorization", format!("Bearer {}", token))
        .body(Body::from(body2))?;
    let resp2: Response = router.clone().oneshot(req2).await?;
    assert_eq!(resp2.status(), StatusCode::CREATED);

    // verify latest pointer and version number
    let doc_after = infra::db::DocumentRepo::find_by_id(&pool, doc.id)
        .await?
        .unwrap();
    assert!(doc_after.current_version_id.is_some());
    let versions = infra::db::VersionRepo::list_by_document_id(&pool, doc.id).await?;
    assert_eq!(versions.len(), 2);
    assert_eq!(versions.first().unwrap().version_number, 2);

    Ok(())
}

#[tokio::test]
async fn document_purge_picks_up_expired_retention_and_cleans_blobs() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let db = std::env::var("DATABASE_URL").expect("DATABASE_URL required for tests");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db)
        .await?;
    sqlx::migrate!().run(&pool).await?;

    // Seed a user
    let user_id = Uuid::new_v4();
    let email = format!("t+{}@example.com", Uuid::new_v4());
    sqlx::query("INSERT INTO users (id, email, roles, status, created_at) VALUES ($1,$2,$3,$4,$5)")
        .bind(user_id)
        .bind(&email)
        .bind(vec!["user".to_string()])
        .bind("active")
        .bind(Utc::now())
        .execute(&pool)
        .await?;

    let now = Utc::now();
    let retention_in_past = now - chrono::Duration::hours(1);

    // Create a document with status 'active' and retention_until in the past (NOT soft-deleted)
    let doc_id = Uuid::new_v4();
    let doc = packdms::domain::models::Document {
        id: doc_id,
        title: "Retention Expired Doc".into(),
        status: "active".into(),
        owner_id: user_id,
        current_version_id: None,
        legal_hold: false,
        retention_until: Some(retention_in_past),
        metadata: serde_json::json!({}),
        created_at: now,
        updated_at: now,
        deleted_at: None,
        deleted_by: None,
        archived_at: None,
        parent_id: None,
    };
    let mut tx = pool.begin().await?;
    packdms::infra::db::DocumentRepo::create(&mut tx, &doc).await?;
    tx.commit().await?;

    // Create a blob referenced by a version
    let blob_id = Uuid::new_v4();
    let blob = packdms::domain::models::Blob {
        id: blob_id,
        storage_key: format!("test/{blob_id}"),
        content_hash: "abc123".into(),
        size_bytes: 42,
        mime_type: "text/plain".into(),
        ref_count: 1,
        status: "active".into(),
        created_at: now,
        purged_at: None,
    };
    let mut tx = pool.begin().await?;
    packdms::infra::db::BlobRepo::create(&mut tx, &blob).await?;
    tx.commit().await?;

    // Create a version linked to the document and blob
    let version_id = Uuid::new_v4();
    let version = packdms::domain::models::DocumentVersion {
        id: version_id,
        document_id: doc_id,
        version_number: 1,
        created_by: user_id,
        storage_key: blob.storage_key.clone(),
        content_hash: "abc123".into(),
        size_bytes: 42,
        mime_type: "text/plain".into(),
        created_at: now,
        status: "active".into(),
        original_filename: "test.txt".into(),
        deleted_at: None,
        deleted_by: None,
        blob_id: Some(blob_id),
    };
    let mut tx = pool.begin().await?;
    packdms::infra::db::VersionRepo::create(&mut tx, &version).await?;
    tx.commit().await?;

    // Update document to point to the version
    sqlx::query("UPDATE documents SET current_version_id = $1 WHERE id = $2")
        .bind(version_id)
        .bind(doc_id)
        .execute(&pool)
        .await?;

    // Run the document purge worker
    packdms::workers::run_document_purge(&pool).await?;

    // Assert: document is now purged
    let purged_doc = packdms::infra::db::DocumentRepo::find_by_id(&pool, doc_id)
        .await?
        .expect("document should still exist in DB");
    assert_eq!(purged_doc.status, "purged", "document should be purged");

    // Assert: blob is marked as pending_deletion
    let updated_blob = packdms::infra::db::BlobRepo::find_by_id(&pool, blob_id)
        .await?
        .expect("blob should still exist in DB");
    assert_eq!(
        updated_blob.status, "pending_deletion",
        "blob should be pending_deletion after purge"
    );
    assert_eq!(updated_blob.ref_count, 0, "blob ref_count should be 0");

    // Assert: audit log entry was created
    let audit_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE action = 'document.retention_purge' AND resource_id = $1",
    )
    .bind(doc_id)
    .fetch_one(&pool)
    .await?;
    assert!(
        audit_count.0 >= 1,
        "expected at least one audit log entry for retention purge"
    );

    Ok(())
}
