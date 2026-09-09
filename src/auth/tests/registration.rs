use super::support::{Fixture, TestBrowser, callback, operation};
use crate::{
    auth::{Provider, credentials::Record, registration::Registration},
    config::{
        authentication::{ClientAuthMethod, OAuthClient},
        schema::SecretReference,
    },
    mcp::AuthenticationProvider,
    trust::secrets::Secret,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn registered(fixture: &Fixture, method: ClientAuthMethod) -> Provider {
    let mut provider: Provider = fixture.provider();
    let configuration: OAuthClient = OAuthClient {
        issuer: fixture.issuer(),
        client_id: "my client:1".to_owned(),
        client_secret: (method != ClientAuthMethod::None)
            .then(|| SecretReference::Environment("APP_SECRET".to_owned())),
        token_endpoint_auth_method: method,
        scopes: vec!["tools:write".to_owned()],
    };
    provider.vault.partition = Some(crate::config::digest(
        &serde_json::to_vec(&configuration).unwrap(),
    ));
    provider.network.registration = Some(Registration {
        configuration,
        secret: (method != ClientAuthMethod::None).then(|| Secret::new("secret :/+".to_owned())),
    });
    fixture.settings.lock().unwrap().authorization["client_id_metadata_document_supported"] =
        json!(false);
    fixture.settings.lock().unwrap().authorization["token_endpoint_auth_methods_supported"] =
        json!([method.name()]);
    provider
}

#[tokio::test]
async fn supplied_clients_skip_registration_and_authenticate_code_and_refresh_exchanges() {
    for method in [
        ClientAuthMethod::None,
        ClientAuthMethod::ClientSecretPost,
        ClientAuthMethod::ClientSecretBasic,
    ] {
        let fixture: Fixture = Fixture::new().await;
        let provider: Provider = registered(&fixture, method);
        let browser: TestBrowser = TestBrowser::new(fixture.issuer());
        provider
            .login_with(callback().await, &browser, || Ok(()), &operation())
            .await
            .unwrap();
        let query: BTreeMap<String, String> = browser.urls.lock().unwrap()[0]
            .query_pairs()
            .into_owned()
            .collect();
        assert_eq!(query["client_id"], "my client:1");
        assert_eq!(query["scope"], "tools:write");
        assert_eq!(query["code_challenge_method"], "S256");
        assert!(!query.contains_key("client_secret"));
        assert_eq!(fixture.count("/register"), 0);
        assert_eq!(
            fixture.provider().status(&operation()).await.unwrap(),
            "signed_out"
        );
        let mut record: Record = provider
            .vault
            .read(&fixture.endpoint, &operation())
            .await
            .unwrap()
            .unwrap();
        record.credential.as_mut().unwrap().expires_at = Some(1);
        provider
            .vault
            .write(&fixture.endpoint, &record, false, &operation())
            .await
            .unwrap();
        fixture.settings.lock().unwrap().token["refresh_token"] = json!("rotated-token");
        provider
            .authorization(&fixture.endpoint, &operation())
            .await
            .unwrap();
        let requests: Vec<String> = fixture.settings.lock().unwrap().requests.clone();
        let tokens: Vec<&String> = requests
            .iter()
            .filter(|request| request.starts_with("POST /token "))
            .collect();
        assert_eq!(tokens.len(), 2);
        for request in tokens {
            check_exchange(method, request);
        }
    }
}

fn check_exchange(method: ClientAuthMethod, request: &str) {
    let (headers, body): (&str, &str) = request.split_once("\r\n\r\n").unwrap();
    let fields: BTreeMap<String, String> = url::form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect();
    assert!(fields.contains_key("resource"));
    match method {
        ClientAuthMethod::None => {
            assert_eq!(fields["client_id"], "my client:1");
            assert!(!fields.contains_key("client_secret"));
            assert!(!headers.to_ascii_lowercase().contains("authorization:"));
        }
        ClientAuthMethod::ClientSecretPost => assert_eq!(fields["client_secret"], "secret :/+"),
        ClientAuthMethod::ClientSecretBasic => {
            use base64::Engine;
            let encoded: String =
                base64::engine::general_purpose::STANDARD.encode("my+client%3A1:secret+%3A%2F%2B");
            assert!(headers.contains(&format!("Basic {encoded}")));
            assert!(!fields.contains_key("client_id"));
            assert!(!fields.contains_key("client_secret"));
        }
    }
}

#[tokio::test]
async fn issuer_mismatch_and_unsupported_method_never_send_client_secrets() {
    for mismatch in [true, false] {
        let fixture: Fixture = Fixture::new().await;
        let provider: Provider = registered(&fixture, ClientAuthMethod::ClientSecretPost);
        if mismatch {
            fixture.settings.lock().unwrap().resource["authorization_servers"] =
                json!(["https://evil.invalid"]);
        } else {
            fixture.settings.lock().unwrap().authorization["token_endpoint_auth_methods_supported"] =
                json!(["none"]);
        }
        let browser: TestBrowser = TestBrowser::new(fixture.issuer());
        assert!(
            provider
                .login_with(callback().await, &browser, || Ok(()), &operation())
                .await
                .is_err()
        );
        assert!(browser.urls.lock().unwrap().is_empty());
        assert_eq!(fixture.count("/token"), 0);
        assert!(
            fixture
                .settings
                .lock()
                .unwrap()
                .requests
                .iter()
                .all(|request| !request.contains("client_secret"))
        );
    }
}

#[test]
fn registration_rejects_invalid_credentials_scopes_and_issuer_configuration() {
    let base: Value = json!({"issuer": "https://issuer.example", "client_id": "app"});
    for (field, replacement) in [
        ("issuer", json!("http://issuer.example")),
        ("issuer", json!("https://user:secret@issuer.example")),
        ("client_id", json!("")),
        ("scopes", json!(["two scopes"])),
        ("client_secret", json!({"env": "SECRET"})),
        ("token_endpoint_auth_method", json!("client_secret_post")),
    ] {
        let mut value: Value = base.clone();
        value[field] = replacement;
        let client: OAuthClient = serde_json::from_value(value).unwrap();
        assert!(client.validate().is_err(), "{field}");
    }
}
