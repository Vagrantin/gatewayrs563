// config.rs
// Configuration module for DavMail Rust

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::collections::HashMap;
use log::{info, error, warn, debug};

pub struct DavMailConfig {
    settings: HashMap<String, String>,
}

impl DavMailConfig {
    pub fn new() -> Self {
        DavMailConfig {
            settings: HashMap::new(),
        }
    }
    
    pub fn load_from_file<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        
        for line in contents.lines() {
            // Skip comments and empty lines
            if line.trim().starts_with('#') || line.trim().is_empty() {
                continue;
            }
            
            // Parse key=value
            if let Some(index) = line.find('=') {
                let key = line[..index].trim().to_string();
                let value = line[index + 1..].trim().to_string();
                self.settings.insert(key, value);
            }
        }
        
        Ok(())
    }
    
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.settings.get(key).cloned()
    }
    
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.settings.get(key).map(|v| {
            v.to_lowercase() == "true" || v == "1" || v.to_lowercase() == "yes"
        })
    }
    
    pub fn get_int(&self, key: &str) -> Option<i32> {
        self.settings.get(key).and_then(|v| v.parse::<i32>().ok())
    }
    
    pub fn set(&mut self, key: &str, value: &str) {
        self.settings.insert(key.to_string(), value.to_string());
    }
    
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        
        // Write header
        writeln!(file, "# DavMail Rust configuration file")?;
        writeln!(file, "# Generated on {}", chrono::Local::now().to_rfc3339())?;
        writeln!(file)?;
        
        // Write settings in sorted order
        let mut keys: Vec<&String> = self.settings.keys().collect();
        keys.sort();
        
        for key in keys {
            if let Some(value) = self.settings.get(key) {
                writeln!(file, "{}={}", key, value)?;
            }
        }
        
        Ok(())
    }
}
