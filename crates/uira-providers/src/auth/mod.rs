pub mod device_flow;
pub mod error;
pub mod pkce;
pub mod providers;
pub mod storage;
pub mod traits;
pub mod types;

#[cfg(feature = "oauth-server")]
pub mod oauth_server;

pub use error::{AuthError, Result};
pub use pkce::{generate_pkce, PkceChallenge};
pub use storage::CredentialStore;
pub use traits::AuthProvider;
pub use types::{AuthMethod, OAuthCallback, OAuthChallenge, OAuthTokens, StoredCredential};

pub use secrecy;

#[cfg(feature = "oauth-server")]
pub use oauth_server::OAuthCallbackServer;
