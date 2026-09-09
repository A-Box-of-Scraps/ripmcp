mod callback;
mod challenge;
pub(crate) mod cleanup;
mod command;
mod credentials;
mod destination;
mod metadata;
mod network;
mod provider;
pub(crate) mod remote;
mod store;
mod token;

#[cfg(test)]
mod tests;

pub use command::run;
pub use provider::Provider;
pub use store::{SecureStore, StoreFuture, SystemStore};

use crate::error::{Error, ErrorKind};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

pub const CLIENT_ID: &str = "https://a-box-of-scraps.github.io/ripmcp/oauth/client.json";
pub const REDIRECT_URI: &str = "http://127.0.0.1:42813/oauth/callback";

fn invalid() -> Error {
    Error::new(
        ErrorKind::Authentication,
        "invalid OAuth response or credential binding",
    )
}

fn required() -> Error {
    Error::new(
        ErrorKind::Authentication,
        "authentication required; run ripmcp auth login <server>",
    )
}

fn unsupported() -> Error {
    Error::new(
        ErrorKind::Unsupported,
        "OAuth provider must support metadata discovery, S256 PKCE, and client metadata documents or native public DCR; pre-registration-only providers are unsupported",
    )
}

fn random() -> Result<String, Error> {
    let mut bytes: [u8; 32] = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| invalid())?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn now() -> Result<u64, Error> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| invalid())
}
