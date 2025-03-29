// DavMail Rust Implementation
// A POP/IMAP/SMTP/CalDav/CardDav/LDAP gateway for Microsoft Exchange/Office 365

use std::sync::{Arc, Mutex};
use std::thread;
use tokio::runtime::Runtime;
use log::{info, error, warn, debug};
use config::{Config, File, Environment};

mod config;
mod exchange;
mod protocols;
mod utils;
mod auth;

// Main application structure
pub struct DavMailRust {
    config: Arc<Config>,
    runtime: Runtime,
    server_handles: Vec<ServerHandle>,
}

// Handle for each protocol server
struct ServerHandle {
    protocol: String,
    handle: Option<thread::JoinHandle<()>>,
    shutdown_signal: Arc<Mutex<bool>>,
}

impl DavMailRust {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Set up configuration
        let config = Config::builder()
            .add_source(File::with_name("davmail.properties").required(false))
            .add_source(Environment::with_prefix("DAVMAIL"))
            .build()?;
        
        let config = Arc::new(config);
        
        // Initialize runtime
        let runtime = Runtime::new()?;
        
        Ok(DavMailRust {
            config,
            runtime,
            server_handles: Vec::new(),
        })
    }
    
    pub fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting DavMail Rust implementation...");
        
        // Check for required configuration
        let exchange_url = self.config.get_string("davmail.url")?;
        info!("Exchange URL: {}", exchange_url);
        
        // Start protocol servers based on configuration
        self.start_protocol_servers()?;
        
        info!("DavMail Rust started successfully");
        Ok(())
    }
    
    fn start_protocol_servers(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Start POP3 server if enabled
        if self.config.get_bool("davmail.popEnabled").unwrap_or(false) {
            let port = self.config.get_int("davmail.popPort").unwrap_or(1110);
            self.start_pop_server(port as u16)?;
        }
        
        // Start IMAP server if enabled
        if self.config.get_bool("davmail.imapEnabled").unwrap_or(false) {
            let port = self.config.get_int("davmail.imapPort").unwrap_or(1143);
            self.start_imap_server(port as u16)?;
        }
        
        // Start SMTP server if enabled
        if self.config.get_bool("davmail.smtpEnabled").unwrap_or(false) {
            let port = self.config.get_int("davmail.smtpPort").unwrap_or(1025);
            self.start_smtp_server(port as u16)?;
        }
        
        // Start CalDAV server if enabled
        if self.config.get_bool("davmail.caldavEnabled").unwrap_or(false) {
            let port = self.config.get_int("davmail.caldavPort").unwrap_or(1080);
            self.start_caldav_server(port as u16)?;
        }
        
        // Start LDAP server if enabled
        if self.config.get_bool("davmail.ldapEnabled").unwrap_or(false) {
            let port = self.config.get_int("davmail.ldapPort").unwrap_or(1389);
            self.start_ldap_server(port as u16)?;
        }
        
        Ok(())
    }
    
    fn start_pop_server(&mut self, port: u16) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting POP3 server on port {}", port);
        let config = self.config.clone();
        let shutdown_signal = Arc::new(Mutex::new(false));
        let shutdown_signal_clone = shutdown_signal.clone();
        
        let handle = thread::spawn(move || {
            let pop_server = protocols::pop::PopServer::new(config, port);
            pop_server.run(shutdown_signal_clone);
        });
        
        self.server_handles.push(ServerHandle {
            protocol: "POP3".to_string(),
            handle: Some(handle),
            shutdown_signal,
        });
        
        Ok(())
    }
    
    fn start_imap_server(&mut self, port: u16) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting IMAP server on port {}", port);
        let config = self.config.clone();
        let shutdown_signal = Arc::new(Mutex::new(false));
        let shutdown_signal_clone = shutdown_signal.clone();
        
        let handle = thread::spawn(move || {
            let imap_server = protocols::imap::ImapServer::new(config, port);
            imap_server.run(shutdown_signal_clone);
        });
        
        self.server_handles.push(ServerHandle {
            protocol: "IMAP".to_string(),
            handle: Some(handle),
            shutdown_signal,
        });
        
        Ok(())
    }
    
    fn start_smtp_server(&mut self, port: u16) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting SMTP server on port {}", port);
        let config = self.config.clone();
        let shutdown_signal = Arc::new(Mutex::new(false));
        let shutdown_signal_clone = shutdown_signal.clone();
        
        let handle = thread::spawn(move || {
            let smtp_server = protocols::smtp::SmtpServer::new(config, port);
            smtp_server.run(shutdown_signal_clone);
        });
        
        self.server_handles.push(ServerHandle {
            protocol: "SMTP".to_string(),
            handle: Some(handle),
            shutdown_signal,
        });
        
        Ok(())
    }
    
    fn start_caldav_server(&mut self, port: u16) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting CalDAV server on port {}", port);
        let config = self.config.clone();
        let shutdown_signal = Arc::new(Mutex::new(false));
        let shutdown_signal_clone = shutdown_signal.clone();
        
        let handle = thread::spawn(move || {
            let caldav_server = protocols::caldav::CalDavServer::new(config, port);
            caldav_server.run(shutdown_signal_clone);
        });
        
        self.server_handles.push(ServerHandle {
            protocol: "CalDAV".to_string(),
            handle: Some(handle),
            shutdown_signal,
        });
        
        Ok(())
    }
    
    fn start_ldap_server(&mut self, port: u16) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting LDAP server on port {}", port);
        let config = self.config.clone();
        let shutdown_signal = Arc::new(Mutex::new(false));
        let shutdown_signal_clone = shutdown_signal.clone();
        
        let handle = thread::spawn(move || {
            let ldap_server = protocols::ldap::LdapServer::new(config, port);
            ldap_server.run(shutdown_signal_clone);
        });
        
        self.server_handles.push(ServerHandle {
            protocol: "LDAP".to_string(),
            handle: Some(handle),
            shutdown_signal,
        });
        
        Ok(())
    }
    
    pub fn shutdown(&mut self) {
        info!("Shutting down DavMail Rust...");
        
        // Signal all servers to shut down
        for server in &self.server_handles {
            let mut shutdown = server.shutdown_signal.lock().unwrap();
            *shutdown = true;
            info!("Sent shutdown signal to {} server", server.protocol);
        }
        
        // Wait for all servers to finish
        for server in &mut self.server_handles {
            if let Some(handle) = server.handle.take() {
                if let Err(e) = handle.join() {
                    error!("Error joining {} server thread: {:?}", server.protocol, e);
                } else {
                    info!("{} server shut down successfully", server.protocol);
                }
            }
        }
        
        info!("DavMail Rust shutdown complete");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();
    
    info!("Initializing DavMail Rust");
    
    // Create and start DavMail
    let mut davmail = DavMailRust::new()?;
    davmail.start()?;
    
    // Wait for termination signal
    let (tx, rx) = std::sync::mpsc::channel();
    ctrlc::set_handler(move || {
        info!("Received termination signal");
        tx.send(()).expect("Failed to send termination signal");
    })?;
    
    rx.recv()?;
    
    // Shutdown
    davmail.shutdown();
    
    Ok(())
}
