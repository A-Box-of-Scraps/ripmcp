use super::{SecureStore, SystemStore};
use crate::{
    cli::Auth,
    config::schema::SecretReference,
    error::{Error, ErrorKind},
    mcp::Operation,
    trust::secrets::{EnvironmentOnly, Secret},
};
use reqwest::header::HeaderValue;
use serde_json::json;

pub(super) fn header(token: &str) -> Result<HeaderValue, Error> {
    if !super::token::bearer(token) {
        return Err(Error::new(
            ErrorKind::Authentication,
            "invalid bearer credential; supply the raw token without the Bearer prefix",
        ));
    }
    let mut value: HeaderValue =
        HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| super::invalid())?;
    value.set_sensitive(true);
    Ok(value)
}

pub(super) async fn execute(
    command: &Auth,
    name: &str,
    reference: &SecretReference,
    operation: &Operation,
) -> Result<(), Error> {
    let state: &str = match command {
        Auth::Login(_) => {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "bearer tokens do not use OAuth login; use ripmcp auth configure <server> --bearer or --bearer-env",
            ));
        }
        Auth::Status(_) => {
            let secret: Secret = reference.resolve(&EnvironmentOnly, operation).await?;
            let _: HeaderValue = header(secret.expose())?;
            "available"
        }
        Auth::Logout(_) => {
            let SecretReference::Stored(key): &SecretReference = reference else {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "credential is externally managed; unset its environment variable or remove it from your keyring; revoke the token at its provider if needed",
                ));
            };
            operation.run(SystemStore.remove(key)).await?;
            if operation.run(SystemStore.read(key)).await?.is_some() {
                return Err(super::invalid());
            }
            "signed_out"
        }
        Auth::Configure(_) => return Err(super::invalid()),
    };
    crate::output::json(
        &mut std::io::stdout().lock(),
        &json!({"schema_version": 1, "auth": {
            "server": name, "state": state, "method": "bearer", "remote_validity_checked": false, "shared_by_endpoint": false
        }}),
    )
}
