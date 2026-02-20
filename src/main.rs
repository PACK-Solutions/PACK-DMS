use axum::Router;
use dotenvy::dotenv;
use sqlx::postgres::PgPoolOptions;
use std::{net::SocketAddr, sync::Arc};
use tower_http::cors::{Any, CorsLayer};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use packdms::infra::auth::{AppState, AuthConfig};
use packdms::infra::storage::FileBlobStore;
use packdms::{api, infra};
use utoipa::OpenApi;
use utoipa_scalar::{Scalar, Servable};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL is required");
    let issuer = std::env::var("JWT_ISSUER").expect("JWT_ISSUER is required");
    let jwks_url = std::env::var("JWKS_URL").expect("JWKS_URL is required");
    let storage_path = std::env::var("STORAGE_PATH").unwrap_or_else(|_| "./data".into());

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;

    let storage =
        Arc::new(FileBlobStore::new(storage_path).await?) as Arc<dyn infra::storage::BlobStore>;

    // Fetch JWKS
    let jwks: jsonwebtoken::jwk::JwkSet = if jwks_url.starts_with("http") {
        let response = reqwest::get(&jwks_url).await?;
        response.json().await?
    } else {
        let content = tokio::fs::read_to_string(&jwks_url).await?;
        serde_json::from_str(&content)?
    };

    let state = Arc::new(AppState {
        pool: pool.clone(),
        auth: AuthConfig { issuer, jwks_url },
        storage,
        jwks: Arc::new(jwks),
    });

    // Build API router and Swagger UI
    let api = api::router(state.clone());
    let openapi = api::ApiDoc::openapi();

    let app = Router::new()
        .merge(api)
        .merge(Scalar::with_url("/docs", openapi))
        .layer(TraceLayer::new_for_http())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(
            CorsLayer::new()
                .allow_methods(Any)
                .allow_origin(Any)
                .allow_headers(Any),
        );

    let addr: SocketAddr = std::env::var("BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
        .parse()?;
    tracing::info!("listening on {}", addr);
    tracing::info!("OpenAPI documentation available at: http://{}/docs", addr);
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}
