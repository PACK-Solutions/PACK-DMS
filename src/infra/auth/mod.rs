use crate::infra::db::UserRepo;
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, request::Parts},
};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AuthConfig {
    pub issuer: String,
    pub jwks_url: String,
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub auth: AuthConfig,
    pub storage: Arc<dyn crate::infra::storage::BlobStore>,
    pub jwks: Arc<jsonwebtoken::jwk::JwkSet>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user id
    pub email: Option<String>,
    pub scope: Option<String>,
    pub exp: usize,
    pub iss: String,
}

#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user_id: Uuid,
    pub email: String,
    pub scopes: Vec<String>,
}

impl AuthContext {
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| s == scope)
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.scopes.iter().any(|s| s == role)
    }

    /// Returns `Ok(())` if the user has the given scope, or a `FORBIDDEN` error.
    pub fn require_scope(&self, scope: &str) -> Result<(), (axum::http::StatusCode, String)> {
        if self.has_scope(scope) {
            Ok(())
        } else {
            Err((
                axum::http::StatusCode::FORBIDDEN,
                format!("Missing {scope} scope"),
            ))
        }
    }
}

#[derive(Debug)]
pub struct JwtAuth(pub AuthContext);

impl<S> FromRequestParts<S> for JwtAuth
where
    S: Send + Sync,
    Arc<AppState>: FromRef<S>,
{
    type Rejection = (StatusCode, String);

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        use axum::extract::FromRef;
        let app: Arc<AppState> = Arc::from_ref(state);
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .cloned();

        async move {
            let Some(header) = header else {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    "Missing Authorization header".to_string(),
                ));
            };
            let auth_header = header
                .to_str()
                .map_err(|_| (StatusCode::UNAUTHORIZED, "Bad header".to_string()))?;
            if !auth_header.starts_with("Bearer ") {
                return Err((StatusCode::UNAUTHORIZED, "Expected Bearer".to_string()));
            }
            let token = &auth_header[7..];

            let header = jsonwebtoken::decode_header(token)
                .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid header".to_string()))?;

            let kid = header
                .kid
                .ok_or((StatusCode::UNAUTHORIZED, "Missing kid".to_string()))?;
            let jwk = app
                .jwks
                .find(&kid)
                .ok_or((StatusCode::UNAUTHORIZED, "Key not found".to_string()))?;

            let decoding_key = DecodingKey::from_jwk(jwk)
                .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid JWK".to_string()))?;

            let mut validation = Validation::new(Algorithm::RS256);
            validation.set_issuer(&[&app.auth.issuer]);
            // You might want to set audience here if you have it in config

            let token_data = decode::<Claims>(token, &decoding_key, &validation)
                .map_err(|e| (StatusCode::UNAUTHORIZED, format!("Invalid token: {}", e)))?;

            let uid = Uuid::parse_str(&token_data.claims.sub)
                .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid sub".to_string()))?;

            let scopes = token_data
                .claims
                .scope
                .unwrap_or_default()
                .split_whitespace()
                .map(|s| s.to_string())
                .collect::<Vec<_>>();

            // Ensure user exists in our DB or auto-provision?
            // The original code checked UserRepo.
            if let Ok(Some(user)) = UserRepo::find_by_id(&app.pool, uid).await {
                Ok(JwtAuth(AuthContext {
                    user_id: user.id,
                    email: user.email,
                    scopes,
                }))
            } else {
                // For now, if user not found, we might want to fail or auto-create.
                // Original behavior was to fail.
                Err((
                    StatusCode::UNAUTHORIZED,
                    "User not found in local database".to_string(),
                ))
            }
        }
    }
}

use jsonwebtoken::{EncodingKey, Header, encode};

pub fn issue_test_jwt(
    private_key_pem: &str,
    kid: &str,
    issuer: &str,
    user_id: Uuid,
    email: &str,
    scope: &str,
    ttl_secs: usize,
) -> anyhow::Result<String> {
    let exp = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as usize)
        + ttl_secs;
    let claims = Claims {
        sub: user_id.to_string(),
        email: Some(email.to_string()),
        scope: Some(scope.to_string()),
        exp,
        iss: issuer.to_string(),
    };
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_string());

    let token = encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(private_key_pem.as_bytes())?,
    )?;
    Ok(token)
}
