use jsonwebtoken::jwk::{JwkSet, Jwk, CommonParameters, RSAKeyParameters, KeyAlgorithm};
use openssl::rsa::Rsa;
use std::fs;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let private_pem_path = "data/keys/private.pem";
    let jwks_path = "data/keys/jwks.json";

    if !Path::new(private_pem_path).exists() {
        println!("Generating new RSA key...");
        let rsa = Rsa::generate(2048)?;
        let private_pem = rsa.private_key_to_pem()?;
        fs::create_dir_all("data/keys")?;
        fs::write(private_pem_path, private_pem)?;
    }

    let private_pem = fs::read(private_pem_path)?;
    let rsa = Rsa::private_key_from_pem(&private_pem)?;
    
    let n = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, rsa.n().to_vec());
    let e = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, rsa.e().to_vec());
    
    let kid = "default-kid";
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
    
    let jwks_json = serde_json::to_string_pretty(&jwks)?;
    fs::write(jwks_path, jwks_json)?;
    println!("JWKS written to {}", jwks_path);
    
    Ok(())
}
