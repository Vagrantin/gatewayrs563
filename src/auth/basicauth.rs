// auth.rs
// Authentication module for DavMail Rust

use std::fmt;

#[derive(Clone)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}


// Don't print the password in debug output
impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}
