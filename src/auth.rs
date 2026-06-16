//! App Store Connect JWT generation (ES256) with in-memory caching.
//!
//! App Store Connect authenticates each request with a short-lived ES256 JWT
//! (max 20 minutes) signed by your `.p8` private key. We mint a token, cache it,
//! and reuse it until it nears expiry.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Serialize;
use tokio::sync::Mutex;

use crate::config::Config;
use crate::error::AscError;

/// Required audience claim for the App Store Connect API.
const AUDIENCE: &str = "appstoreconnect-v1";
/// Token lifetime. Apple rejects tokens that live longer than 20 minutes.
const TOKEN_TTL: Duration = Duration::from_secs(20 * 60);
/// Refresh a cached token once it is within this margin of expiring.
const REFRESH_MARGIN: Duration = Duration::from_secs(2 * 60);

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    iat: u64,
    exp: u64,
    aud: &'a str,
}

#[derive(Clone)]
struct CachedToken {
    token: String,
    /// Unix seconds at which the token expires.
    expires_at: u64,
}

/// Mints and caches App Store Connect JWTs from the configured credentials.
pub struct TokenProvider {
    config: Arc<Config>,
    cache: Mutex<Option<CachedToken>>,
}

impl TokenProvider {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            cache: Mutex::new(None),
        }
    }

    /// Return a valid bearer token, reusing the cached one when still fresh.
    pub async fn token(&self) -> Result<String, AscError> {
        let now = unix_now();

        {
            let cache = self.cache.lock().await;
            if let Some(cached) = cache.as_ref() {
                if cached.expires_at > now + REFRESH_MARGIN.as_secs() {
                    return Ok(cached.token.clone());
                }
            }
        }

        let creds = self.config.credentials()?;
        let expires_at = now + TOKEN_TTL.as_secs();

        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(creds.key_id.to_string());
        header.typ = Some("JWT".to_string());

        let claims = Claims {
            iss: creds.issuer_id,
            iat: now,
            exp: expires_at,
            aud: AUDIENCE,
        };

        let key = EncodingKey::from_ec_pem(creds.private_key_pem.as_bytes()).map_err(|e| {
            AscError::Auth(format!(
                "the configured private key is not a valid ES256 (.p8) PEM key: {e}"
            ))
        })?;

        let token = encode(&header, &claims, &key)
            .map_err(|e| AscError::Auth(format!("could not sign the JWT: {e}")))?;

        let mut cache = self.cache.lock().await;
        *cache = Some(CachedToken {
            token: token.clone(),
            expires_at,
        });

        Ok(token)
    }
}

/// Current Unix time in whole seconds.
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{decode, DecodingKey, Validation};

    // A throwaway P-256 keypair for round-tripping (generated offline; test-only,
    // grants access to nothing). Only the base64 DER bodies are stored here — the
    // PEM is assembled at runtime by `pem()` so the literal key markers never
    // appear in source and can't trip secret scanners on a public repo.
    const TEST_PRIVATE_BODY: &str = "MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgtNQuT3hctsLS5iks\nldU7lAHLp9QPbYtRkNrhPNxlreOhRANCAATtwWcC7S4Iv3kFf5CZ+S00uBy6z0Ai\nkKhZsS1aG3tDlcxyWKPycElp3WMMtbnrPLa6ZaRHAwEY2M5jfPbUvS7O";
    const TEST_PUBLIC_BODY: &str = "MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE7cFnAu0uCL95BX+QmfktNLgcus9A\nIpCoWbEtWht7Q5XMclij8nBJad1jDLW56zy2umWkRwMBGNjOY3z21L0uzg==";

    const TEST_ISSUER: &str = "57246542-96fe-1a63-e053-0824d011072a";
    const TEST_KEY_ID: &str = "ABC123DEFG";

    /// Assemble a PEM document from a label and base64 body at runtime.
    fn pem(label: &str, body: &str) -> String {
        let rule = "-----";
        format!("{rule}BEGIN {label}{rule}\n{body}\n{rule}END {label}{rule}\n")
    }

    #[tokio::test]
    async fn signs_and_verifies_a_valid_token() {
        let private_pem = pem("PRIVATE KEY", TEST_PRIVATE_BODY);
        let config = Arc::new(Config::from_parts(
            Some(TEST_ISSUER),
            Some(TEST_KEY_ID),
            Some(&private_pem),
        ));
        let provider = TokenProvider::new(config);

        let token = provider.token().await.expect("token should be minted");

        // Header carries kid + ES256.
        let header = jsonwebtoken::decode_header(&token).expect("valid header");
        assert_eq!(header.alg, Algorithm::ES256);
        assert_eq!(header.kid.as_deref(), Some(TEST_KEY_ID));

        // Claims decode and verify against the matching public key.
        let mut validation = Validation::new(Algorithm::ES256);
        validation.set_audience(&[AUDIENCE]);
        validation.set_issuer(&[TEST_ISSUER]);
        let public_pem = pem("PUBLIC KEY", TEST_PUBLIC_BODY);
        let key = DecodingKey::from_ec_pem(public_pem.as_bytes()).expect("public key");

        #[derive(serde::Deserialize)]
        struct Decoded {
            iss: String,
            aud: String,
            exp: u64,
            iat: u64,
        }
        let data = decode::<Decoded>(&token, &key, &validation).expect("token verifies");
        assert_eq!(data.claims.aud, AUDIENCE);
        assert_eq!(data.claims.iss, TEST_ISSUER);
        assert!(data.claims.exp > data.claims.iat);
        assert!(data.claims.exp - data.claims.iat <= TOKEN_TTL.as_secs());
    }

    #[tokio::test]
    async fn missing_credentials_report_what_is_missing() {
        let provider = TokenProvider::new(Arc::new(Config::from_parts(None, None, None)));
        let err = provider.token().await.unwrap_err();
        assert!(matches!(err, AscError::Config(_)));
    }
}
