// protocols/imap.rs
// IMAP protocol implementation for DavMail Rust

use std::sync::{Arc, Mutex};
use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write, BufReader, BufRead};
use std::thread;
use log::{info, error, warn, debug};
use config::Config;

use crate::exchange::client::ExchangeClient;
use crate::auth::Credentials;

pub struct ImapServer {
    config: Arc<Config>,
    port: u16,
}

impl ImapServer {
    pub fn new(config: Arc<Config>, port: u16) -> Self {
        ImapServer { config, port }
    }
    
    pub fn run(&self, shutdown_signal: Arc<Mutex<bool>>) {
        // Bind to the IMAP port
        let listener = match TcpListener::bind(format!("0.0.0.0:{}", self.port)) {
            Ok(listener) => listener,
            Err(e) => {
                error!("Failed to bind IMAP server to port {}: {}", self.port, e);
                return;
            }
        };
        
        // Set timeout for accept operations to allow checking shutdown signal
        listener.set_nonblocking(true).unwrap();
        
        info!("IMAP server listening on port {}", self.port);
        
        loop {
            // Check if shutdown was requested
            if *shutdown_signal.lock().unwrap() {
                info!("IMAP server shutdown requested");
                break;
            }
            
            // Accept new connections
            match listener.accept() {
                Ok((stream, addr)) => {
                    info!("New IMAP connection from {}", addr);
                    let config = self.config.clone();
                    thread::spawn(move || {
                        if let Err(e) = handle_imap_client(stream, config) {
                            error!("Error handling IMAP client: {}", e);
                        }
                    });
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No connection available, wait a bit
                    thread::sleep(std::time::Duration::from_millis(100));
                    continue;
                }
                Err(e) => {
                    error!("Error accepting IMAP connection: {}", e);
                    break;
                }
            }
        }
        
        info!("IMAP server stopped");
    }
}

fn handle_imap_client(mut stream: TcpStream, config: Arc<Config>) -> Result<(), Box<dyn std::error::Error>> {
    // Set TCP keepalive
    stream.set_keepalive(Some(std::time::Duration::from_secs(60)))?;
    
    // Send greeting
    writeln!(stream, "* OK [CAPABILITY IMAP4rev1 LITERAL+ SASL-IR LOGIN-REFERRALS AUTH=PLAIN AUTH=LOGIN] DavMail Rust IMAP ready")?;
    
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    let mut authenticated = false;
    let mut selected_mailbox: Option<String> = None;
    let mut exchange_client: Option<ExchangeClient> = None;
    
    // Process client commands
    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            // Connection closed
            break;
        }
        
        debug!("IMAP received: {}", line.trim());
        
        // Parse IMAP command
        let parts: Vec<&str> = line.trim().splitn(3, ' ').collect();
        if parts.len() < 2 {
            writeln!(stream, "* BAD Invalid command")?;
            continue;
        }
        
        let tag = parts[0];
        let command = parts[1].to_uppercase();
        
        match command.as_str() {
            "CAPABILITY" => {
                writeln!(stream, "* CAPABILITY IMAP4rev1 LITERAL+ SASL-IR LOGIN-REFERRALS AUTH=PLAIN AUTH=LOGIN")?;
                writeln!(stream, "{} OK CAPABILITY completed", tag)?;
            },
            
            "LOGIN" => {
                if parts.len() < 3 {
                    writeln!(stream, "{} BAD Missing credentials", tag)?;
                    continue;
                }
                
                // Parse username/password
                let auth_parts: Vec<&str> = parts[2].splitn(2, ' ').collect();
                if auth_parts.len() != 2 {
                    writeln!(stream, "{} BAD Invalid credentials format", tag)?;
                    continue;
                }
                
                let username = auth_parts[0].trim_matches('"');
                let password = auth_parts[1].trim_matches('"');
                
                // Create Exchange client and authenticate
                let credentials = Credentials::new(username.to_string(), password.to_string());
                let exchange_url = config.get_string("davmail.url").unwrap_or_default();
                
                match ExchangeClient::new(&exchange_url, credentials) {
                    Ok(client) => {
                        exchange_client = Some(client);
                        authenticated = true;
                        writeln!(stream, "{} OK LOGIN completed", tag)?;
                    },
                    Err(e) => {
                        error!("Authentication failed: {}", e);
                        writeln!(stream, "{} NO LOGIN failed", tag)?;
                    }
                }
            },
            
            "LIST" => {
                if !authenticated {
                    writeln!(stream, "{} NO Not authenticated", tag)?;
                    continue;
                }
                
                // Get reference and mailbox name
                let list_args = if parts.len() >= 3 {
                    parts[2].splitn(2, ' ').collect::<Vec<&str>>()
                } else {
                    vec!["", ""]
                };
                
                let reference = list_args.get(0).unwrap_or(&"").trim_matches('"');
                let mailbox_pattern = list_args.get(1).unwrap_or(&"*").trim_matches('"');
                
                // List mailboxes from Exchange
                if let Some(client) = &exchange_client {
                    match client.list_folders(reference, mailbox_pattern) {
                        Ok(folders) => {
                            for folder in folders {
                                writeln!(stream, "* LIST (\\HasNoChildren) \"/\" \"{}\"", folder)?;
                            }
                            writeln!(stream, "{} OK LIST completed", tag)?;
                        },
                        Err(e) => {
                            error!("LIST command failed: {}", e);
                            writeln!(stream, "{} NO LIST failed", tag)?;
                        }
                    }
                } else {
                    writeln!(stream, "{} NO Exchange client not initialized", tag)?;
                }
            },
            
            "SELECT" => {
                if !authenticated {
                    writeln!(stream, "{} NO Not authenticated", tag)?;
                    continue;
                }
                
                if parts.len() < 3 {
                    writeln!(stream, "{} BAD Missing mailbox name", tag)?;
                    continue;
                }
                
                let mailbox = parts[2].trim_matches('"');
                
                if let Some(client) = &exchange_client {
                    match client.select_folder(mailbox) {
                        Ok(stats) => {
                            selected_mailbox = Some(mailbox.to_string());
                            
                            writeln!(stream, "* {} EXISTS", stats.exists)?;
                            writeln!(stream, "* {} RECENT", stats.recent)?;
                            writeln!(stream, "* OK [UNSEEN {}] First unseen message", stats.unseen)?;
                            writeln!(stream, "* OK [UIDVALIDITY {}] UIDs valid", stats.uid_validity)?;
                            writeln!(stream, "* OK [UIDNEXT {}] Predicted next UID", stats.uid_next)?;
                            writeln!(stream, "* FLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft)")?;
                            writeln!(stream, "* OK [PERMANENTFLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft \\*)]")?;
                            writeln!(stream, "{} OK [READ-WRITE] SELECT completed", tag)?;
                        },
                        Err(e) => {
                            error!("SELECT command failed: {}", e);
                            writeln!(stream, "{} NO SELECT failed", tag)?;
                        }
                    }
                } else {
                    writeln!(stream, "{} NO Exchange client not initialized", tag)?;
                }
            },
            
            "FETCH" => {
                if !authenticated {
                    writeln!(stream, "{} NO Not authenticated", tag)?;
                    continue;
                }
                
                if selected_mailbox.is_none() {
                    writeln!(stream, "{} NO No mailbox selected", tag)?;
                    continue;
                }
                
                if parts.len() < 3 {
                    writeln!(stream, "{} BAD Missing fetch arguments", tag)?;
                    continue;
                }
                
                // Parse sequence set and fetch items
                let fetch_args = parts[2].splitn(2, ' ').collect::<Vec<&str>>();
                if fetch_args.len() != 2 {
                    writeln!(stream, "{} BAD Invalid fetch arguments", tag)?;
                    continue;
                }
                
                let sequence_set = fetch_args[0];
                let items = fetch_args[1];
                
                if let Some(client) = &exchange_client {
                    match client.fetch_messages(selected_mailbox.as_ref().unwrap(), sequence_set, items) {
                        Ok(messages) => {
                            for message in messages {
                                writeln!(stream, "* {} FETCH {}", message.sequence, message.data)?;
                            }
                            writeln!(stream, "{} OK FETCH completed", tag)?;
                        },
                        Err(e) => {
                            error!("FETCH command failed: {}", e);
                            writeln!(stream, "{} NO FETCH failed", tag)?;
                        }
                    }
                } else {
                    writeln!(stream, "{} NO Exchange client not initialized", tag)?;
                }
            },
            
            "LOGOUT" => {
                writeln!(stream, "* BYE IMAP session terminating")?;
                writeln!(stream, "{} OK LOGOUT completed", tag)?;
                break;
            },
            
            _ => {
                writeln!(stream, "{} BAD Command not implemented", tag)?;
            }
        }
        
        stream.flush()?;
    }
    
    Ok(())
}
