mod lifecycle;
mod metadata;
mod registration;
pub(super) mod support;

use super::{
    Provider,
    callback::Callback,
    credentials::{Credential, Record, Vault},
    metadata::Profile,
    store::SecureStore,
};
use crate::{
    error::{Error, ErrorKind},
    mcp::{AuthenticationProvider, Operation},
};
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::Ordering},
};
use support::{Fixture, TestBrowser, callback, operation};

async fn login(fixture: &Fixture) -> Provider {
    let provider: Provider = fixture.provider();
    provider
        .login_with(
            callback().await,
            &TestBrowser::new(fixture.issuer()),
            || Ok(()),
            &operation(),
        )
        .await
        .unwrap();
    provider
}

#[tokio::test]
async fn login_persists_bound_credentials_and_pkce() {
    let fixture: Fixture = Fixture::new().await;
    let browser: TestBrowser = TestBrowser::new(fixture.issuer());
    let provider: Provider = fixture.provider();
    let callback: Callback = callback().await;
    let verifier: String = callback.verifier.clone();
    provider
        .login_with(callback, &browser, || Ok(()), &operation())
        .await
        .unwrap();
    assert_eq!(provider.status(&operation()).await.unwrap(), "saved");
    let urls: std::sync::MutexGuard<'_, Vec<url::Url>> = browser.urls.lock().unwrap();
    let query: BTreeMap<String, String> = urls[0].query_pairs().into_owned().collect();
    assert_eq!(query["client_id"], super::CLIENT_ID);
    assert_eq!(query["resource"], fixture.endpoint.as_str());
    assert_eq!(query["code_challenge_method"], "S256");
    assert_eq!(query["state"].len(), 43);
    assert_eq!(query["code_challenge"].len(), 43);
    assert_ne!(query["code_challenge"], verifier);
    let settings: std::sync::MutexGuard<'_, support::Settings> = fixture.settings.lock().unwrap();
    let token: &String = settings
        .requests
        .iter()
        .find(|request| request.starts_with("POST /token "))
        .unwrap();
    assert!(token.contains(&format!("code_verifier={verifier}")));
    assert!(token.contains("resource="));
    assert!(
        settings
            .requests
            .iter()
            .all(|request| !request.to_ascii_lowercase().contains("authorization:"))
    );
}

#[tokio::test]
async fn callback_failures_never_exchange_codes_or_report_saved() {
    for (key, value) in [
        ("state", "wrong"),
        ("iss", "https://evil.invalid"),
        ("iss", "REMOVE"),
        ("code", "REMOVE"),
        ("error", "access_denied"),
    ] {
        let fixture: Fixture = Fixture::new().await;
        let provider: Provider = fixture.provider();
        let mut browser: TestBrowser = TestBrowser::new(fixture.issuer());
        browser.change.insert(key.to_owned(), value.to_owned());
        let error: Error = provider
            .login_with(callback().await, &browser, || Ok(()), &operation())
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Authentication);
        assert!(!error.to_string().contains("secret"));
        assert_eq!(fixture.count("/token"), 0);
        assert_eq!(provider.status(&operation()).await.unwrap(), "signed_out");
    }
}

#[tokio::test]
async fn secure_store_failure_before_browser_and_after_exchange_fails_login() {
    for failure in [1, 2] {
        let fixture: Fixture = Fixture::new().await;
        fixture.memory.fail_write.store(failure, Ordering::SeqCst);
        let browser: TestBrowser = TestBrowser::new(fixture.issuer());
        let provider: Provider = fixture.provider();
        assert!(
            provider
                .login_with(callback().await, &browser, || Ok(()), &operation())
                .await
                .is_err()
        );
        assert_eq!(browser.urls.lock().unwrap().len(), failure - 1);
        assert_eq!(provider.status(&operation()).await.unwrap(), "signed_out");
    }
}

#[tokio::test]
async fn logout_during_login_prevents_resurrection() {
    let fixture: Fixture = Fixture::new().await;
    let provider: Arc<Provider> = Arc::new(fixture.provider());
    let mut browser: TestBrowser = TestBrowser::new(fixture.issuer());
    browser.logout = Some(provider.clone());
    assert!(
        provider
            .login_with(callback().await, &browser, || Ok(()), &operation())
            .await
            .is_err()
    );
    assert_eq!(fixture.count("/token"), 0);
    assert_eq!(provider.status(&operation()).await.unwrap(), "signed_out");
}

#[tokio::test]
async fn logout_invalidates_other_providers_without_caching() {
    let fixture: Fixture = Fixture::new().await;
    let provider: Provider = login(&fixture).await;
    let other: Provider = fixture.provider();
    assert!(
        other
            .authorization(&fixture.endpoint, &operation())
            .await
            .unwrap()
            .is_some()
    );
    provider.logout(&operation()).await.unwrap();
    assert!(
        other
            .authorization(&fixture.endpoint, &operation())
            .await
            .is_err()
    );
    assert_eq!(other.status(&operation()).await.unwrap(), "signed_out");
    let data: std::sync::MutexGuard<'_, BTreeMap<String, Vec<u8>>> =
        fixture.memory.data.lock().unwrap();
    assert!(
        data.values()
            .all(|bytes| !String::from_utf8_lossy(bytes).contains("secret-"))
    );
}

#[tokio::test]
async fn endpoint_and_issuer_changes_cannot_receive_credentials() {
    let fixture: Fixture = Fixture::new().await;
    let provider: Provider = login(&fixture).await;
    let changed: url::Url = fixture.endpoint.join("other").unwrap();
    assert!(
        provider
            .authorization(&changed, &operation())
            .await
            .is_err()
    );
    fixture.settings.lock().unwrap().authorization["issuer"] = json!("https://evil.invalid");
    assert!(
        provider
            .authorization(&fixture.endpoint, &operation())
            .await
            .is_err()
    );
    assert_eq!(fixture.count("/token"), 1);
}

async fn expire(fixture: &Fixture) {
    let key: String = Vault::key(&fixture.endpoint);
    let bytes: Vec<u8> = fixture.memory.read(&key).await.unwrap().unwrap();
    let mut record: Record = serde_json::from_slice(&bytes).unwrap();
    record.credential.as_mut().unwrap().expires_at = Some(0);
    fixture
        .memory
        .write(&key, &serde_json::to_vec(&record).unwrap(), false)
        .await
        .unwrap();
    fixture.settings.lock().unwrap().token["refresh_token"] = json!("rotated-refresh");
}

#[tokio::test]
async fn refresh_is_serialized_and_atomic_across_providers() {
    let fixture: Fixture = Fixture::new().await;
    let first: Provider = login(&fixture).await;
    let second: Provider = fixture.provider();
    expire(&fixture).await;
    let operation: Operation = operation();
    type AuthorizationResult = Result<Option<HeaderValue>, Error>;
    let (a, b): (AuthorizationResult, AuthorizationResult) = tokio::join!(
        first.authorization(&fixture.endpoint, &operation),
        second.authorization(&fixture.endpoint, &operation)
    );
    assert!(a.unwrap().is_some());
    assert!(b.unwrap().is_some());
    assert_eq!(fixture.count("/token"), 2);
    assert_eq!(first.status(&operation).await.unwrap(), "saved");
}

#[tokio::test]
async fn failed_refresh_cannot_replay_old_token() {
    let fixture: Fixture = Fixture::new().await;
    let provider: Provider = login(&fixture).await;
    expire(&fixture).await;
    fixture.settings.lock().unwrap().token_status = "400 Bad Request";
    assert!(
        provider
            .authorization(&fixture.endpoint, &operation())
            .await
            .is_err()
    );
    assert!(
        provider
            .authorization(&fixture.endpoint, &operation())
            .await
            .is_err()
    );
    assert_eq!(fixture.count("/token"), 2);
    assert_eq!(provider.status(&operation()).await.unwrap(), "signed_out");
}

#[tokio::test]
async fn bad_metadata_fails_before_browser() {
    for field in [
        "issuer",
        "code_challenge_methods_supported",
        "client_id_metadata_document_supported",
        "token_endpoint",
    ] {
        let fixture: Fixture = Fixture::new().await;
        fixture.settings.lock().unwrap().authorization[field] = Value::Null;
        let browser: TestBrowser = TestBrowser::new("issuer".to_owned());
        assert!(
            fixture
                .provider()
                .login_with(callback().await, &browser, || Ok(()), &operation())
                .await
                .is_err()
        );
        assert!(browser.urls.lock().unwrap().is_empty());
        assert_eq!(fixture.count("/token"), 0);
    }
}

#[tokio::test]
async fn metadata_fallback_and_challenge_scopes_are_supported() {
    let fixture: Fixture = Fixture::new().await;
    {
        let mut settings: std::sync::MutexGuard<'_, support::Settings> =
            fixture.settings.lock().unwrap();
        settings.replies.insert(
            "/.well-known/oauth-protected-resource/mcp".to_owned(),
            ("404 Not Found".to_owned(), "{}".to_owned()),
        );
        settings.replies.insert(
            "/.well-known/oauth-authorization-server".to_owned(),
            ("404 Not Found".to_owned(), "{}".to_owned()),
        );
        settings.challenge = Some("Basic realm=\"test\", Bearer scope=\"tools:write\"".to_owned());
    }
    let profile: Profile = fixture
        .provider()
        .network
        .discover(&fixture.endpoint, &operation())
        .await
        .unwrap();
    assert_eq!(profile.scope.as_deref(), Some("tools:write"));
    assert_eq!(fixture.count("/.well-known/openid-configuration"), 1);
    assert_eq!(fixture.count("/.well-known/oauth-protected-resource"), 1);
}

#[test]
fn challenges_reject_duplicates_and_malformed_quotes() {
    for challenge in [
        "Bearer scope=\"a\", scope=\"b\"",
        "Bearer resource_metadata=\"unterminated",
        "Bearer scope=a, Bearer scope=b",
    ] {
        let mut headers: HeaderMap = HeaderMap::new();
        headers.insert(
            "www-authenticate",
            HeaderValue::from_str(challenge).unwrap(),
        );
        assert!(super::challenge::parse(&headers).is_err());
    }
}

#[test]
fn bearer_challenges_can_follow_other_schemes_with_token68() {
    let mut headers: HeaderMap = HeaderMap::new();
    headers.insert(
        "www-authenticate",
        HeaderValue::from_static(
            "Negotiate abc/+==, Basic realm=\"a,b\", Bearer scope=\"read write\",",
        ),
    );
    let challenge: super::challenge::Challenge = super::challenge::parse(&headers).unwrap();
    assert_eq!(challenge.scope.as_deref(), Some("read write"));
}

#[tokio::test]
async fn resource_binding_rejects_other_endpoint() {
    let fixture: Fixture = Fixture::new().await;
    fixture.settings.lock().unwrap().resource["resource"] = json!("https://other.invalid/mcp");
    assert!(
        fixture
            .provider()
            .network
            .discover(&fixture.endpoint, &operation())
            .await
            .is_err()
    );
}

#[test]
fn production_oauth_rejects_insecure_urls() {
    for value in [
        "http://127.0.0.1/mcp",
        "https://user:password@example.org/mcp",
        "https://example.org/mcp#fragment",
        "https://example.org/mcp?secret=1",
    ] {
        assert!(Provider::new(url::Url::parse(value).unwrap()).is_err());
    }
}

#[tokio::test]
async fn saved_binding_is_not_authorized_by_opaque_key_alone() {
    let fixture: Fixture = Fixture::new().await;
    let provider: Provider = login(&fixture).await;
    let key: String = Vault::key(&fixture.endpoint);
    let bytes: Vec<u8> = fixture.memory.read(&key).await.unwrap().unwrap();
    let mut record: Record = serde_json::from_slice(&bytes).unwrap();
    let credential: &mut Credential = record.credential.as_mut().unwrap();
    credential.resource = "https://other.invalid/mcp".to_owned();
    fixture
        .memory
        .write(&key, &serde_json::to_vec(&record).unwrap(), false)
        .await
        .unwrap();
    assert!(
        provider
            .authorization(&fixture.endpoint, &operation())
            .await
            .is_err()
    );
}
