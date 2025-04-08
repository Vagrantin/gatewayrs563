// exchange/client.rs
// Exchange Web Services (EWS) client implementation

use std::error::Error;
use std::fmt;
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, AUTHORIZATION};
use tokio::runtime::Runtime;
use log::{debug, error, info};
use regex;

use crate::auth::*;

#[derive(Debug)]
pub enum ExchangeError {
    HttpError(reqwest::Error),
    AuthError(String),
    ParseError(String),
    ConfigError(String),
    RuntimeError(String),
}

impl fmt::Display for ExchangeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ExchangeError::HttpError(e) => write!(f, "HTTP error: {}", e),
            ExchangeError::AuthError(s) => write!(f, "Authentication error: {}", s),
            ExchangeError::ParseError(s) => write!(f, "Parse error: {}", s),
            ExchangeError::ConfigError(s) => write!(f, "Configuration error: {}", s),
            ExchangeError::RuntimeError(s) => write!(f, "Runtime error: {}", s),
        }
    }
}

impl Error for ExchangeError {}

impl From<reqwest::Error> for ExchangeError {
    fn from(error: reqwest::Error) -> Self {
        ExchangeError::HttpError(error)
    }
}

#[derive(Debug)]
pub struct FolderStats {
    pub exists: u32,
    pub recent: u32,
    pub unseen: u32,
    pub uid_validity: u32,
    pub uid_next: u32,
}

#[derive(Debug)]
pub struct Message {
    pub sequence: u32,
    pub data: String,
}

pub enum AuthMethod {
    Basic(BasicAuth),
    OAuth2(OAuth2Auth),
}

pub struct ExchangeClient {
    base_url: String,
    client: Client,
    auth_method: AuthMethod,
    token: Option<String>,
    runtime: Runtime,
}

impl ExchangeClient {
        pub async fn new_with_basic_auth(base_url: &str, username: &'static str, password: &'static str) -> Result<Self, ExchangeError> {
            if base_url.is_empty() {
                return Err(ExchangeError::ConfigError("Exchange URL not configured".to_string()));
            }

            let client = Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?;

            let auth_method = AuthMethod::Basic(BasicAuth::new(username, password));

            let runtime = Runtime::new()
                .map_err(|e| ExchangeError::RuntimeError(format!("Failed to create Tokio runtime: {}", e)))?;

            let mut exchange_client = ExchangeClient {
                base_url: base_url.to_string(),
                client,
                auth_method,
                token: None,
                runtime,
            };

            // Authenticate immediately
            exchange_client.authenticate().await;

            Ok(exchange_client)
    }
    pub async fn new_with_oauth2(base_url: &str, oauth2_config: OAuth2Config) -> Result<Self, ExchangeError> {
        if base_url.is_empty() {
            return Err(ExchangeError::ConfigError("Exchange URL not configured".to_string()));
        }
        
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        
        let auth_method = AuthMethod::OAuth2(OAuth2Auth::new(oauth2_config).unwrap());
        
        let runtime = Runtime::new()
            .map_err(|e| ExchangeError::RuntimeError(format!("Failed to create Tokio runtime: {}", e)))?;
        
        let mut exchange_client = ExchangeClient {
            base_url: base_url.to_string(),
            client,
            auth_method,
            token: None,
            runtime,
        };
        
        // Authenticate immediately
        exchange_client.authenticate().await;
        
        Ok(exchange_client)
    }
    
    async fn authenticate(&mut self) -> Result<(), ExchangeError> {
        debug!("Authenticating to Exchange server: {}", self.base_url);

        match &mut self.auth_method {
            AuthMethod::Basic(basic_auth) => {
                self.token = Some(basic_auth.get_auth_header()
                    .map_err(|e| ExchangeError::AuthError(e.to_string()))?);
                self.verify_basic_auth().await?;
            },
            AuthMethod::OAuth2(oauth2_auth) => {
                // We need to block on the async call to get the OAuth2 token
                let token = self.runtime.block_on(async {
                    oauth2_auth.async_get_auth_header().await
                }).unwrap();
                self.token = Some(token);
            }
        }

        debug!("Authentication successful");
        Ok(())
    }

    async fn verify_basic_auth(&self) -> Result<(), ExchangeError> {
        // Only needed for basic auth to verify credentials
        debug!("Verifying basic authentication credentials");

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/xml; charset=utf-8"));
        headers.insert(AUTHORIZATION, HeaderValue::from_str(self.token.as_ref().unwrap())
            .map_err(|e| ExchangeError::AuthError(e.to_string()))?);

        let response = self.client
            .post(format!("{}/EWS/Exchange.asmx", self.base_url))
            .headers(headers)
            .body(r#"<?xml version="1.0" encoding="utf-8"?>
                <soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"
                               xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
                  <soap:Body>
                    <FindFolder xmlns="http://schemas.microsoft.com/exchange/services/2006/messages"
                               xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types"
                               Traversal="Shallow">
                      <FolderShape>
                        <t:BaseShape>IdOnly</t:BaseShape>
                      </FolderShape>
                      <ParentFolderIds>
                        <t:DistinguishedFolderId Id="inbox"/>
                      </ParentFolderIds>
                    </FindFolder>
                  </soap:Body>
                </soap:Envelope>"#)
            .send().await?;

        if !response.status().is_success() {
            return Err(ExchangeError::AuthError(format!("Authentication failed with status code: {}", response.status())));
        }

        Ok(())
    }
    
    // Refreshes the authentication token if necessary
    fn ensure_authenticated(&mut self) -> Result<(), ExchangeError> {
        match &mut self.auth_method {
            AuthMethod::Basic(_) => {
                // Basic auth doesn't expire, so nothing to do
                Ok(())
            },
            AuthMethod::OAuth2(oauth2_auth) => {
                // Refresh the OAuth2 token if needed
                let token = self.runtime.block_on(async {
                    oauth2_auth.async_get_auth_header().await
                }).unwrap();
                self.token = Some(token);
                Ok(())
            }
        }
    }

    
    pub async fn list_folders(&self, reference: &str, pattern: &str) -> Result<Vec<String>, ExchangeError> {
        // Ensure we have a valid authentication token
        self.ensure_authenticated()?;

        debug!("Listing folders with reference '{}' and pattern '{}'", reference, pattern);

        // Prepare headers
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/xml; charset=utf-8"));
        headers.insert(AUTHORIZATION, HeaderValue::from_str(self.token.as_ref().unwrap())
            .map_err(|e| ExchangeError::AuthError(e.to_string()))?);

        // Build the EWS FindFolder request
        let parent_folder: String = if reference.is_empty() {
            // If reference is empty, use msgfolderroot
            format!(r#"<t:DistinguishedFolderId Id="msgfolderroot"/>"#)
        } else {
            // Otherwise use the specified folder ID
            format!(r#"<t:FolderId Id="{}"/>"#, reference)
        };

        let body = format!(r#"<?xml version="1.0" encoding="utf-8"?>
            <soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"
                           xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
              <soap:Body>
                <FindFolder xmlns="http://schemas.microsoft.com/exchange/services/2006/messages"
                           xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types"
                           Traversal="Deep">
                  <FolderShape>
                    <t:BaseShape>Default</t:BaseShape>
                  </FolderShape>
                  <ParentFolderIds>
                    {}
                  </ParentFolderIds>
                </FindFolder>
              </soap:Body>
            </soap:Envelope>"#, parent_folder);

        // Send the request
        let response = self.client
            .post(format!("{}/EWS/Exchange.asmx", self.base_url))
            .headers(headers)
            .body(body)
            .send().await?;

        if !response.status().is_success() {
            return Err(ExchangeError::HttpError(
                reqwest::Error::from(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Request failed with status: {}", response.status())
                ))
            ));
        }

        let response_text = response.text().await;

        // In a real implementation, you would parse the XML response
        // For this example, we'll return simulated folders
        if pattern == "*" {
            Ok(vec![
                "INBOX".to_string(),
                "Sent Items".to_string(),
                "Drafts".to_string(),
                "Deleted Items".to_string(),
                "Junk Email".to_string(),
                "Archive".to_string(),
            ])
        } else {
            // Filter folders based on pattern (simple wildcard implementation)
            let pattern = pattern.replace("*", ".*");
            let regex = regex::Regex::new(&pattern).map_err(|e| {
                ExchangeError::ParseError(format!("Invalid pattern: {}", e))
            })?;

            let all_folders = vec![
                "INBOX".to_string(),
                "Sent Items".to_string(),
                "Drafts".to_string(),
                "Deleted Items".to_string(),
                "Junk Email".to_string(),
                "Archive".to_string(),
            ];

            Ok(all_folders.into_iter()
                .filter(|folder| regex.is_match(folder))
                .collect())
        }
    }
    
    pub async fn select_folder(&self, folder_name: &str) -> Result<FolderStats, ExchangeError> {
        debug!("Selecting folder: {}", folder_name);
        
        // Prepare headers
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/xml; charset=utf-8"));
        headers.insert(AUTHORIZATION, HeaderValue::from_str(self.token.as_ref().unwrap()).unwrap());
        
        // Determine folder ID (distinguished or by name)
        let folder_id = match folder_name.to_uppercase().as_str() {
            "INBOX" => r#"<t:DistinguishedFolderId Id="inbox"/>"#.to_string(),
            "SENT" | "SENT ITEMS" => r#"<t:DistinguishedFolderId Id="sentitems"/>"#.to_string(),
            "DRAFTS" => r#"<t:DistinguishedFolderId Id="drafts"/>"#.to_string(),
            "TRASH" | "DELETED ITEMS" => r#"<t:DistinguishedFolderId Id="deleteditems"/>"#.to_string(),
            _ => {
                // For other folders, we would need to find the folder ID first
                // This is simplified for this example
                format!(r#"<t:DistinguishedFolderId Id="msgfolderroot"/>
                         <t:Folders>
                           <t:Folder>
                             <t:DisplayName>{}</t:DisplayName>
                           </t:Folder>
                         </t:Folders>"#, folder_name)
            },
        };
        
        // Build the EWS GetFolder request
        let body = format!(r#"<?xml version="1.0" encoding="utf-8"?>
            <soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"
                           xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
              <soap:Body>
                <GetFolder xmlns="http://schemas.microsoft.com/exchange/services/2006/messages"
                          xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
                  <FolderShape>
                    <t:BaseShape>Default</t:BaseShape>
                    <t:AdditionalProperties>
                      <t:FieldURI FieldURI="folder:TotalCount"/>
                      <t:FieldURI FieldURI="folder:UnreadCount"/>
                    </t:AdditionalProperties>
                  </FolderShape>
                  <FolderIds>
                    {}
                  </FolderIds>
                </GetFolder>
              </soap:Body>
            </soap:Envelope>"#, folder_id);
        
        // Send the request
        let response = self.client
            .post(format!("{}/EWS/Exchange.asmx", self.base_url))
            .headers(headers)
            .body(body)
            .send().await?;
        
        if !response.status().is_success() {
            return Err(ExchangeError::HttpError(
                reqwest::Error::from(std::io::Error::new(
                    std::io::ErrorKind::Other, 
                    format!("Request failed with status: {}", response.status())
                ))
            ));
        }
        
        let response_text = response.text().await;
        
        // In a real implementation, you would parse the XML response
        // For this example, we'll return simulated stats
        // In a production environment, parse the XML response to get the actual values
        
        // Generate a deterministic UID validity based on folder name
        let uid_validity = folder_name.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
        
        Ok(FolderStats {
            exists: 125,          // Total messages in folder
            recent: 5,            // New messages since last check
            unseen: 10,           // Unread messages
            uid_validity,         // A unique identifier for the folder state
            uid_next: 1000,       // Next UID to be assigned
        })
    }
    
    pub async fn fetch_messages(&self, folder: &str, sequence_set: &str, items: &str) 
        -> Result<Vec<Message>, ExchangeError> {
        debug!("Fetching messages from folder '{}', sequence '{}', items '{}'", 
               folder, sequence_set, items);
        
        // Prepare headers
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/xml; charset=utf-8"));
        headers.insert(AUTHORIZATION, HeaderValue::from_str(self.token.as_ref().unwrap()).unwrap());
        
        // Parse sequence set (e.g., "1:10", "1,3,5", "*")
        let sequences = parse_sequence_set(sequence_set)?;
        
        // Determine folder ID
        let folder_id = match folder.to_uppercase().as_str() {
            "INBOX" => "inbox".to_string(),
            "SENT" | "SENT ITEMS" => "sentitems".to_string(),
            "DRAFTS" => "drafts".to_string(),
            "TRASH" | "DELETED ITEMS" => "deleteditems".to_string(),
            _ => folder.to_string(),
        };
        
        // Build the EWS FindItem request
        // In a real implementation, you would need to handle paging for large result sets
        let body = format!(r#"<?xml version="1.0" encoding="utf-8"?>
            <soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"
                          xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
              <soap:Body>
                <FindItem xmlns="http://schemas.microsoft.com/exchange/services/2006/messages"
                         xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types"
                         Traversal="Shallow">
                  <ItemShape>
                    <t:BaseShape>IdOnly</t:BaseShape>
                    <t:AdditionalProperties>
                      <t:FieldURI FieldURI="item:Subject"/>
                      <t:FieldURI FieldURI="item:DateTimeReceived"/>
                      <t:FieldURI FieldURI="message:From"/>
                      <t:FieldURI FieldURI="message:IsRead"/>
                    </t:AdditionalProperties>
                  </ItemShape>
                  <IndexedPageItemView MaxEntriesReturned="100" Offset="0" BasePoint="Beginning"/>
                  <ParentFolderIds>
                    <t:DistinguishedFolderId Id="{}"/>
                  </ParentFolderIds>
                </FindItem>
              </soap:Body>
            </soap:Envelope>"#, folder_id);
        
        // Send the request
        let response = self.client
            .post(format!("{}/EWS/Exchange.asmx", self.base_url))
            .headers(headers)
            .body(body)
            .send().await?;
        
        if !response.status().is_success() {
            return Err(ExchangeError::HttpError(
                reqwest::Error::from(std::io::Error::new(
                    std::io::ErrorKind::Other, 
                    format!("Request failed with status: {}", response.status())
                ))
            ));
        }
        
        let response_text = response.text().await;
        
        // In a real implementation, you would parse the XML response and build IMAP responses
        // For this example, we'll simulate messages
        
        // Parse the items requested (e.g., "BODY[HEADER] FLAGS UID")
        let fetch_items: Vec<&str> = items.trim_matches(|c| c == '(' || c == ')').split_whitespace().collect();
        
        let mut result = Vec::new();
        for &seq in &sequences {
            // Generate message data based on requested items
            let mut data_parts = Vec::new();
            
            for item in &fetch_items {
                match *item {
                    "FLAGS" => {
                        data_parts.push("FLAGS (\\Seen)".to_string());
                    },
                    "UID" => {
                        let uid = 1000 + seq;
                        data_parts.push(format!("UID {}", uid));
                    },
                    item if item.starts_with("BODY[HEADER]") => {
                        data_parts.push(format!("BODY[HEADER] {{320}}\r\nFrom: user{}@example.com\r\nTo: recipient@example.com\r\nSubject: Test message {}\r\nDate: Fri, 28 Mar 2025 10:{}:00 +0000\r\nMessage-ID: <{}.{}.{}@example.com>\r\n\r\n", 
                                               seq % 10, seq, seq % 60, seq, seq, seq));
                    },
                    item if item.starts_with("BODY[TEXT]") => {
                        data_parts.push(format!("BODY[TEXT] {{42}}\r\nThis is the body of test message {}.\r\n", seq));
                    },
                    item if item == "BODY[]" || item.starts_with("BODY[") => {
                        data_parts.push(format!("BODY[] {{362}}\r\nFrom: user{}@example.com\r\nTo: recipient@example.com\r\nSubject: Test message {}\r\nDate: Fri, 28 Mar 2025 10:{}:00 +0000\r\nMessage-ID: <{}.{}.{}@example.com>\r\n\r\nThis is the body of test message {}.\r\n", 
                                               seq % 10, seq, seq % 60, seq, seq, seq, seq));
                    },
                    _ => {
                        // Ignore unsupported items
                    }
                }
            }
            
            if !data_parts.is_empty() {
                let data = format!("({})", data_parts.join(" "));
                result.push(Message {
                    sequence: seq,
                    data,
                });
            }
        }
        
        Ok(result)
    }
}

// Helper function to parse an IMAP sequence set
fn parse_sequence_set(sequence_set: &str) -> Result<Vec<u32>, ExchangeError> {
    let mut result = Vec::new();
    
    for part in sequence_set.split(',') {
        if part == "*" {
            // For simplicity, treat "*" as "all messages" - in this case we'll return IDs 1-10
            for i in 1..=10 {
                result.push(i);
            }
        } else if part.contains(':') {
            // Range, e.g., "1:5"
            let range_parts: Vec<&str> = part.split(':').collect();
            if range_parts.len() != 2 {
                return Err(ExchangeError::ParseError(format!("Invalid range: {}", part)));
            }
            
            let start = if range_parts[0] == "*" {
                // In a real implementation, this would be the highest message number
                10
            } else {
                range_parts[0].parse::<u32>().map_err(|_| {
                    ExchangeError::ParseError(format!("Invalid sequence number: {}", range_parts[0]))
                })?
            };
            
            let end = if range_parts[1] == "*" {
                // In a real implementation, this would be the highest message number
                10
            } else {
                range_parts[1].parse::<u32>().map_err(|_| {
                    ExchangeError::ParseError(format!("Invalid sequence number: {}", range_parts[1]))
                })?
            };
            
            for i in start.min(end)..=start.max(end) {
                result.push(i);
            }
        } else {
            // Single message number
            let num = part.parse::<u32>().map_err(|_| {
                ExchangeError::ParseError(format!("Invalid sequence number: {}", part))
            })?;
            result.push(num);
        }
    }
    
    Ok(result)
}
