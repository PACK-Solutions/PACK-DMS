use chrono::Utc;
use packdms::{api, infra};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use uuid::Uuid;
use axum::{http::{Request, StatusCode}, body::Body, response::Response};
use tower::util::ServiceExt; // for `oneshot`
use jsonwebtoken::jwk::{JwkSet, Jwk, CommonParameters, RSAKeyParameters, KeyAlgorithm};

#[tokio::test]
async fn upload_version_updates_current_and_increments() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let db = std::env::var("DATABASE_URL").expect("DATABASE_URL required for tests");
    let pool = PgPoolOptions::new().max_connections(1).connect(&db).await?;
    sqlx::migrate!().run(&pool).await?;

    // Setup RSA keys for testing
    let rsa_key = openssl::rsa::Rsa::generate(2048)?;
    let private_pem = String::from_utf8(rsa_key.private_key_to_pem()?)?;
    // public_pem is not needed if we build JWK directly from rsa_key

    // Create a JWK from the public key
    let n = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, rsa_key.n().to_vec());
    let e = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, rsa_key.e().to_vec());
    
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
        .bind(&vec!["user".to_string()])
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
    };
    infra::db::DocumentRepo::create(&mut tx, &doc).await?;
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
    fn multipart_body(boundary: &str, name: &str, filename: &str, content_type: &str, content: &str) -> (String, Vec<u8>) {
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
