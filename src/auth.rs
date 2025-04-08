// auth.rs
// Authentication module for DavMail Rust
use std::fmt;

pub mod basicauth;
pub mod oauth2;

pub use basicauth::*;
pub use oauth2::*;


// Auth provider trait to support multiple authentication methods
pub trait AuthProvider {
    fn get_auth_header(&self) -> Result<String, Box<dyn std::error::Error>>;
}

// Basic Auth implementation
pub struct BasicAuth {
    pub username: &'static str,
    pub password: &'static str,
}

// Want to have it in the module.....
impl BasicAuth {
    pub fn new(username: &'static str, password: &'static str) -> Self {
        BasicAuth { username, password }
    }
}


impl AuthProvider for BasicAuth {
    fn get_auth_header(&self) -> Result<String, Box<dyn std::error::Error>> {
        let auth = format!("{}:{}", self.username, self.password);
        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, auth.as_bytes());
        Ok(format!("Basic {}", encoded))
    }
}

// Don't print the password in debug output
impl fmt::Debug for BasicAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

// OAuth2 Auth implementation
pub struct OAuth2Auth {
    client: OAuth2Client,
}

impl OAuth2Auth {
    pub fn new(config: OAuth2Config) -> Result<Self, OAuth2Error> {
        let client = OAuth2Client::new(config)?;
        Ok(Self { client })
    }
}

impl AuthProvider for OAuth2Auth {
    fn get_auth_header(&self) -> Result<String, Box<dyn std::error::Error>> {
        // In a real implementation, this would be async
        // For synchronous API compatibility, we'd need to use tokio::runtime::Runtime
        // to block on the async operation
        Err("OAuth2Auth.get_auth_header() requires async runtime, use async_get_auth_header() instead".into())
    }
}

impl OAuth2Auth {
    // Async version of get_auth_header
    pub async fn async_get_auth_header(&mut self) -> Result<String, OAuth2Error> {
        let token = self.client.get_token().await?;
        Ok(token.authorization_header())
    }
}
