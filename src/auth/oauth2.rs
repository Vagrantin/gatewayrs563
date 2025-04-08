// auth/oauth2.rs
// OAuth2 implementation for Exchange Web Services (EWS)

use std::error::Error;
use std::fmt;
use std::time::{Duration, SystemTime};
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, ACCEPT};
use serde::{Serialize, Deserialize};
use log::debug;

// OAuth2 configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2Config {
    pub tenant_id: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub scope: String,
    pub authority: String,
}

impl OAuth2Config {
    pub fn new(
        tenant_id: &str,
        client_id: &str,
        client_secret: &str,
        redirect_uri: &str,
        scope: &str,
    ) -> Self {
        // Default authority is Microsoft's OAuth2 endpoint
        let authority = format!("https://login.microsoftonline.com/{}", tenant_id);
        
        Self {
            tenant_id: tenant_id.to_string(),
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            redirect_uri: redirect_uri.to_string(),
            scope: scope.to_string(),
            authority,
        }
    }
    
    pub fn with_authority(mut self, authority: &str) -> Self {
        self.authority = authority.to_string();
        self
    }
}

// OAuth2 token response structure
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
    pub id_token: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

// OAuth2 error types
#[derive(Debug)]
pub enum OAuth2Error {
    RequestError(reqwest::Error),
    ResponseError(String),
    ParseError(String),
    TokenExpired,
    ConfigError(String),
}

impl fmt::Display for OAuth2Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            OAuth2Error::RequestError(e) => write!(f, "OAuth2 request error: {}", e),
            OAuth2Error::ResponseError(s) => write!(f, "OAuth2 response error: {}", s),
            OAuth2Error::ParseError(s) => write!(f, "OAuth2 parse error: {}", s),
            OAuth2Error::TokenExpired => write!(f, "OAuth2 token expired"),
            OAuth2Error::ConfigError(s) => write!(f, "OAuth2 configuration error: {}", s),
        }
    }
}

impl Error for OAuth2Error {}

impl From<reqwest::Error> for OAuth2Error {
    fn from(error: reqwest::Error) -> Self {
        OAuth2Error::RequestError(error)
    }
}

// OAuth2 token with metadata
#[derive(Debug, Clone)]
pub struct OAuth2Token {
    pub access_token: String,
    pub token_type: String,
    pub expires_at: SystemTime,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
}

impl OAuth2Token {
    fn from_response(response: TokenResponse) -> Self {
        let now = SystemTime::now();
        let expires_at = now + Duration::from_secs(response.expires_in);
        
        Self {
            access_token: response.access_token,
            token_type: response.token_type,
            expires_at,
            refresh_token: response.refresh_token,
            scope: response.scope,
        }
    }
    
    pub fn is_expired(&self) -> bool {
        match SystemTime::now().duration_since(self.expires_at) {
            Ok(_) => true,  // Current time is after expiry time
            Err(_) => false, // Current time is before expiry time
        }
    }
    
    pub fn is_expiring_soon(&self, buffer_seconds: u64) -> bool {
        let buffer = Duration::from_secs(buffer_seconds);
        match SystemTime::now().duration_since(self.expires_at.checked_sub(buffer).unwrap_or(self.expires_at)) {
            Ok(_) => true,  // Token will expire within buffer time
            Err(_) => false, // Token won't expire within buffer time
        }
    }
    
    pub fn authorization_header(&self) -> String {
        format!("{} {}", self.token_type, self.access_token)
    }
}

// OAuth2 client
pub struct OAuth2Client {
    config: OAuth2Config,
    http_client: Client,
    current_token: Option<OAuth2Token>,
}

impl OAuth2Client {
    pub fn new(config: OAuth2Config) -> Result<Self, OAuth2Error> {
        // Validate configuration
        if config.tenant_id.is_empty() {
            return Err(OAuth2Error::ConfigError("Tenant ID cannot be empty".to_string()));
        }
        if config.client_id.is_empty() {
            return Err(OAuth2Error::ConfigError("Client ID cannot be empty".to_string()));
        }
        if config.client_secret.is_empty() {
            return Err(OAuth2Error::ConfigError("Client secret cannot be empty".to_string()));
        }
        if config.scope.is_empty() {
            return Err(OAuth2Error::ConfigError("Scope cannot be empty".to_string()));
        }
        
        let http_client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
        
        Ok(Self {
            config,
            http_client,
            current_token: None,
        })
    }
    
    // Acquire a token using client credentials grant flow
    pub async fn acquire_token_client_credentials(&mut self) -> Result<OAuth2Token, OAuth2Error> {
        debug!("Acquiring OAuth2 token using client credentials flow");
        
        let token_endpoint = format!("{}/oauth2/v2.0/token", self.config.authority);
        
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/x-www-form-urlencoded"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        
        let form_params = [
            ("grant_type", "client_credentials"),
            ("client_id", &self.config.client_id),
            ("client_secret", &self.config.client_secret),
            ("scope", &self.config.scope),
        ];
        
        let response = self.http_client
            .post(&token_endpoint)
            .headers(headers)
            .form(&form_params)
            .send()
            .await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Could not read error response".to_string());
            return Err(OAuth2Error::ResponseError(format!("Token request failed ({}): {}", status, error_text)));
        }
        
        let token_response: TokenResponse = response.json().await?;
        
        // Check for errors in the response
        if let Some(error) = token_response.error {
            let description = token_response.error_description.unwrap_or_else(|| "No error description".to_string());
            return Err(OAuth2Error::ResponseError(format!("OAuth error: {} - {}", error, description)));
        }
        
        let token = OAuth2Token::from_response(token_response);
        self.current_token = Some(token.clone());
        
        debug!("Successfully acquired OAuth2 token, expires at {:?}", token.expires_at);
        Ok(token)
    }
    
    // Acquire a token using authorization code grant flow
    pub async fn acquire_token_by_authorization_code(&mut self, code: &str) -> Result<OAuth2Token, OAuth2Error> {
        debug!("Acquiring OAuth2 token using authorization code flow");
        
        let token_endpoint = format!("{}/oauth2/v2.0/token", self.config.authority);
        
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/x-www-form-urlencoded"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        
        let form_params = [
            ("grant_type", "authorization_code"),
            ("client_id", &self.config.client_id),
            ("client_secret", &self.config.client_secret),
            ("code", code),
            ("redirect_uri", &self.config.redirect_uri),
            ("scope", &self.config.scope),
        ];
        
        let response = self.http_client
            .post(&token_endpoint)
            .headers(headers)
            .form(&form_params)
            .send()
            .await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Could not read error response".to_string());
            return Err(OAuth2Error::ResponseError(format!("Token request failed ({}): {}", status, error_text)));
        }
        
        let token_response: TokenResponse = response.json().await?;
        
        // Check for errors in the response
        if let Some(error) = token_response.error {
            let description = token_response.error_description.unwrap_or_else(|| "No error description".to_string());
            return Err(OAuth2Error::ResponseError(format!("OAuth error: {} - {}", error, description)));
        }
        
        let token = OAuth2Token::from_response(token_response);
        self.current_token = Some(token.clone());
        
        debug!("Successfully acquired OAuth2 token, expires at {:?}", token.expires_at);
        Ok(token)
    }
    
    // Refresh an existing token
    pub async fn refresh_token(&mut self, refresh_token: &str) -> Result<OAuth2Token, OAuth2Error> {
        debug!("Refreshing OAuth2 token");
        
        let token_endpoint = format!("{}/oauth2/v2.0/token", self.config.authority);
        
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/x-www-form-urlencoded"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        
        let form_params = [
            ("grant_type", "refresh_token"),
            ("client_id", &self.config.client_id),
            ("client_secret", &self.config.client_secret),
            ("refresh_token", refresh_token),
            ("scope", &self.config.scope),
        ];
        
        let response = self.http_client
            .post(&token_endpoint)
            .headers(headers)
            .form(&form_params)
            .send()
            .await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Could not read error response".to_string());
            return Err(OAuth2Error::ResponseError(format!("Token refresh failed ({}): {}", status, error_text)));
        }
        
        let token_response: TokenResponse = response.json().await?;
        
        // Check for errors in the response
        if let Some(error) = token_response.error {
            let description = token_response.error_description.unwrap_or_else(|| "No error description".to_string());
            return Err(OAuth2Error::ResponseError(format!("OAuth error: {} - {}", error, description)));
        }
        
        let token = OAuth2Token::from_response(token_response);
        self.current_token = Some(token.clone());
        
        debug!("Successfully refreshed OAuth2 token, expires at {:?}", token.expires_at);
        Ok(token)
    }
    
    // Get a valid token, refreshing if necessary
    pub async fn get_token(&mut self) -> Result<OAuth2Token, OAuth2Error> {
        if let Some(token) = &self.current_token.clone() {
            // If token is expiring soon (within 5 minutes), refresh it
            if token.is_expiring_soon(300) {
                debug!("Current token is expiring soon, refreshing");
                if let Some(refresh_token) = &token.refresh_token {
                    return self.refresh_token(refresh_token).await;
                } else {
                    debug!("No refresh token available, acquiring new token");
                    return self.acquire_token_client_credentials().await;
                }
            }
            
            debug!("Using existing OAuth2 token");
            return Ok(token.clone());
        }
        
        // No token yet, acquire a new one
        debug!("No current token, acquiring new token");
        self.acquire_token_client_credentials().await
    }
    
    // Generate authorization URL for user to visit
    pub fn get_authorization_url(&self, state: &str) -> String {
        format!(
            "{}/oauth2/v2.0/authorize?client_id={}&response_type=code&redirect_uri={}&scope={}&state={}",
            self.config.authority,
            self.config.client_id,
            urlencoding::encode(&self.config.redirect_uri),
            urlencoding::encode(&self.config.scope),
            urlencoding::encode(state)
        )
    }
}
