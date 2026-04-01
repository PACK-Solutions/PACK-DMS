use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde::Serialize;
use std::fs;
use uuid::Uuid;

#[derive(Serialize)]
struct Claims {
    sub: String,
    email: Option<String>,
    scope: Option<String>,
    exp: usize,
    iss: String,
}

fn main() -> anyhow::Result<()> {
    let private_pem_path = "data/keys/private.pem";
    let private_pem = fs::read_to_string(private_pem_path).map_err(|e| {
        anyhow::anyhow!(
            "Cannot read {private_pem_path}: {e}. Run `cargo run --example gen_jwks` first."
        )
    })?;

    let issuer =
        std::env::var("JWT_ISSUER").unwrap_or_else(|_| "https://example.com/auth".to_string());
    let kid = "default-kid";
    let ttl_secs: usize = 3600;

    let exp = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as usize)
        + ttl_secs;

    // Regular user token
    let user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001")?;
    let user_claims = Claims {
        sub: user_id.to_string(),
        email: Some("user@example.com".to_string()),
        scope: Some("document:read document:write".to_string()),
        exp,
        iss: issuer.clone(),
    };
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_string());
    let user_token = encode(
        &header,
        &user_claims,
        &EncodingKey::from_rsa_pem(private_pem.as_bytes())?,
    )?;

    // Admin token
    let admin_id = Uuid::parse_str("00000000-0000-0000-0000-000000000002")?;
    let admin_claims = Claims {
        sub: admin_id.to_string(),
        email: Some("admin@example.com".to_string()),
        scope: Some("document:read document:write admin".to_string()),
        exp,
        iss: issuer,
    };
    let admin_token = encode(
        &header,
        &admin_claims,
        &EncodingKey::from_rsa_pem(private_pem.as_bytes())?,
    )?;

    println!("Tokens valid for {ttl_secs} seconds (1 hour)\n");
    println!("=== User Token (user@example.com, scopes: document:read document:write) ===");
    println!("{user_token}\n");
    println!("=== Admin Token (admin@example.com, scopes: document:read document:write admin) ===");
    println!("{admin_token}\n");

    // Create or update api-requests/http-client.env.json with fresh tokens
    let env_path = "api-requests/http-client.env.json";
    let template = format!(
        r#"{{
  "dev": {{
    "host": "localhost:8080",
    "auth_token": "{user_token}",
    "admin_token": "{admin_token}"
  }}
}}
"#
    );

    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(env_path).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(env_path, &template)?;
    println!("Wrote {env_path} with fresh tokens.");

    Ok(())
}
