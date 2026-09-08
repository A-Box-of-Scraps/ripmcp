use super::{
    expire, login,
    support::{Fixture, TestBrowser, callback, operation},
};
use crate::{
    auth::{Provider, credentials::Vault, store::SecureStore},
    deadline::Deadline,
    error::{Error, ErrorKind},
    mcp::{AuthenticationProvider, CancellationToken, Operation},
};
use reqwest::{
    StatusCode,
    header::{HeaderMap, HeaderValue},
};
use serde_json::json;
use std::{collections::BTreeMap, sync::atomic::Ordering, time::Duration};

#[tokio::test]
async fn stale_secure_store_write_after_logout_is_not_authoritative() {
    let fixture: Fixture = Fixture::new().await;
    let provider: Provider = login(&fixture).await;
    let key: String = Vault::key(&fixture.endpoint);
    let previous: Vec<u8> = fixture.memory.read(&key).await.unwrap().unwrap();
    provider.logout(&operation()).await.unwrap();
    fixture.memory.write(&key, &previous, false).await.unwrap();
    assert!(
        fixture
            .provider()
            .authorization(&fixture.endpoint, &operation())
            .await
            .is_err()
    );
    assert_eq!(provider.status(&operation()).await.unwrap(), "signed_out");
    provider.logout(&operation()).await.unwrap();
    let bytes: Vec<u8> = fixture.memory.read(&key).await.unwrap().unwrap();
    assert!(!String::from_utf8(bytes).unwrap().contains("secret-"));
}

#[tokio::test]
async fn refresh_persistence_failure_keeps_credentials_revoked() {
    let fixture: Fixture = Fixture::new().await;
    let provider: Provider = login(&fixture).await;
    expire(&fixture).await;
    let next = fixture.memory.writes.load(Ordering::SeqCst) + 2;
    fixture.memory.fail_write.store(next, Ordering::SeqCst);
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
}

#[tokio::test]
async fn logout_failure_does_not_report_success() {
    let fixture: Fixture = Fixture::new().await;
    let provider: Provider = login(&fixture).await;
    fixture.memory.fail_write.store(
        fixture.memory.writes.load(Ordering::SeqCst) + 1,
        Ordering::SeqCst,
    );
    assert!(provider.logout(&operation()).await.is_err());
    assert!(
        provider
            .authorization(&fixture.endpoint, &operation())
            .await
            .is_err()
    );
    fixture.memory.fail_write.store(0, Ordering::SeqCst);
    provider.logout(&operation()).await.unwrap();
}

#[tokio::test]
async fn secure_store_read_failure_never_means_signed_out_success() {
    let fixture: Fixture = Fixture::new().await;
    fixture.memory.fail_read.store(true, Ordering::SeqCst);
    let provider: Provider = fixture.provider();
    assert!(provider.status(&operation()).await.is_err());
    assert!(provider.logout(&operation()).await.is_err());
    assert!(
        provider
            .authorization(&fixture.endpoint, &operation())
            .await
            .is_err()
    );
    assert_eq!(fixture.count("/mcp"), 0);
}

#[tokio::test]
async fn challenged_scopes_wait_for_explicit_login_without_replaying_requests() {
    let fixture: Fixture = Fixture::new().await;
    let provider: Provider = login(&fixture).await;
    let header: Option<HeaderValue> = provider
        .authorization(&fixture.endpoint, &operation())
        .await
        .unwrap();
    let mut headers: HeaderMap = HeaderMap::new();
    headers.insert(
        "www-authenticate",
        HeaderValue::from_static("Bearer error=\"insufficient_scope\", scope=\"tools:write\""),
    );
    assert!(
        provider
            .challenged_with_authorization(
                &fixture.endpoint,
                StatusCode::FORBIDDEN,
                &headers,
                header.as_ref(),
                &operation()
            )
            .await
            .is_err()
    );
    assert_eq!(
        provider.status(&operation()).await.unwrap(),
        "login_required"
    );
    assert!(
        provider
            .authorization(&fixture.endpoint, &operation())
            .await
            .is_err()
    );
    assert_eq!(fixture.count("/token"), 1);
    let browser: TestBrowser = TestBrowser::new(fixture.issuer());
    provider
        .login_with(callback().await, &browser, || Ok(()), &operation())
        .await
        .unwrap();
    let scopes: BTreeMap<String, String> = browser.urls.lock().unwrap()[0]
        .query_pairs()
        .into_owned()
        .collect();
    assert_eq!(scopes["scope"], "tools:read tools:write");
    assert_eq!(provider.status(&operation()).await.unwrap(), "saved");
}

#[tokio::test]
async fn late_challenge_cannot_restore_logged_out_credentials() {
    let fixture: Fixture = Fixture::new().await;
    let provider: Provider = login(&fixture).await;
    let header: Option<HeaderValue> = provider
        .authorization(&fixture.endpoint, &operation())
        .await
        .unwrap();
    provider.logout(&operation()).await.unwrap();
    assert!(
        provider
            .challenged_with_authorization(
                &fixture.endpoint,
                StatusCode::UNAUTHORIZED,
                &HeaderMap::new(),
                header.as_ref(),
                &operation()
            )
            .await
            .is_err()
    );
    assert_eq!(provider.status(&operation()).await.unwrap(), "signed_out");
}

#[tokio::test]
async fn callback_replay_cannot_complete_a_new_login() {
    let fixture: Fixture = Fixture::new().await;
    let provider: Provider = fixture.provider();
    let mut browser: TestBrowser = TestBrowser::new(fixture.issuer());
    provider
        .login_with(callback().await, &browser, || Ok(()), &operation())
        .await
        .unwrap();
    let query: BTreeMap<String, String> = browser.urls.lock().unwrap()[0]
        .query_pairs()
        .into_owned()
        .collect();
    browser
        .change
        .insert("state".to_owned(), query["state"].clone());
    assert!(
        provider
            .login_with(callback().await, &browser, || Ok(()), &operation())
            .await
            .is_err()
    );
    assert_eq!(fixture.count("/token"), 1);
}

#[tokio::test]
async fn cancellation_and_timeout_release_callback_and_locks() {
    let fixture: Fixture = Fixture::new().await;
    let provider: Provider = fixture.provider();
    let profile: crate::auth::metadata::Profile = provider
        .network
        .discover(&fixture.endpoint, &operation())
        .await
        .unwrap();
    for cancelled in [false, true] {
        let callback: crate::auth::callback::Callback = callback().await;
        let redirect: url::Url = url::Url::parse(&callback.redirect).unwrap();
        let token: CancellationToken = CancellationToken::new();
        if cancelled {
            token.cancel();
        }
        let operation: Operation = Operation::new(Deadline::new(Duration::from_millis(20)), token);
        let error: Error = callback.receive(&profile, &operation).await.unwrap_err();
        assert_eq!(
            error.kind,
            if cancelled {
                ErrorKind::Cancelled
            } else {
                ErrorKind::Timeout
            }
        );
        drop(callback);
        let listener: tokio::net::TcpListener =
            tokio::net::TcpListener::bind(("127.0.0.1", redirect.port().unwrap()))
                .await
                .unwrap();
        drop(listener);
    }
    let operation: Operation = operation();
    let lock: std::fs::File = provider
        .vault
        .lock(&fixture.endpoint, &operation)
        .await
        .unwrap();
    let short: Operation = Operation::new(
        Deadline::new(Duration::from_millis(20)),
        CancellationToken::new(),
    );
    assert_eq!(
        provider
            .vault
            .lock(&fixture.endpoint, &short)
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Timeout
    );
    drop(lock);
    provider
        .vault
        .lock(&fixture.endpoint, &operation)
        .await
        .unwrap();
}

#[tokio::test]
async fn changed_project_snapshot_prevents_token_exchange() {
    let fixture: Fixture = Fixture::new().await;
    let checks: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let result: Result<(), Error> = fixture
        .provider()
        .login_with(
            callback().await,
            &TestBrowser::new(fixture.issuer()),
            || {
                if checks.fetch_add(1, Ordering::SeqCst) == 0 {
                    Ok(())
                } else {
                    Err(Error::new(
                        ErrorKind::Configuration,
                        "configuration changed",
                    ))
                }
            },
            &operation(),
        )
        .await;
    assert_eq!(result.unwrap_err().kind, ErrorKind::Configuration);
    assert_eq!(fixture.count("/token"), 0);
}

#[tokio::test]
async fn denied_consent_is_explicit_and_does_not_exchange() {
    let fixture: Fixture = Fixture::new().await;
    let mut browser: TestBrowser = TestBrowser::new(fixture.issuer());
    browser.change = BTreeMap::from([
        ("code".to_owned(), "REMOVE".to_owned()),
        ("error".to_owned(), "access_denied".to_owned()),
    ]);
    let error: Error = fixture
        .provider()
        .login_with(callback().await, &browser, || Ok(()), &operation())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("denied"));
    assert_eq!(fixture.count("/token"), 0);
}

#[tokio::test]
async fn absent_issuer_allowed_only_when_not_advertised() {
    let fixture: Fixture = Fixture::new().await;
    fixture.settings.lock().unwrap().authorization["authorization_response_iss_parameter_supported"] =
        json!(false);
    let mut browser: TestBrowser = TestBrowser::new(fixture.issuer());
    browser.change.insert("iss".to_owned(), "REMOVE".to_owned());
    fixture
        .provider()
        .login_with(callback().await, &browser, || Ok(()), &operation())
        .await
        .unwrap();
}

#[tokio::test]
async fn uncertain_refresh_times_out_once_and_requires_login() {
    let fixture: Fixture = Fixture::new().await;
    let provider: Provider = login(&fixture).await;
    expire(&fixture).await;
    fixture.settings.lock().unwrap().stall_path = Some("/token".to_owned());
    let short: Operation = Operation::new(
        Deadline::new(Duration::from_millis(300)),
        CancellationToken::new(),
    );
    assert_eq!(
        provider
            .authorization(&fixture.endpoint, &short)
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Timeout
    );
    assert!(
        provider
            .authorization(&fixture.endpoint, &operation())
            .await
            .is_err()
    );
    assert_eq!(fixture.count("/token"), 2);
}

struct PendingWrite(std::sync::Arc<super::support::Memory>);

impl SecureStore for PendingWrite {
    fn read<'a>(&'a self, key: &'a str) -> crate::auth::StoreFuture<'a, Option<Vec<u8>>> {
        self.0.read(key)
    }
    fn write<'a>(&'a self, _: &'a str, _: &'a [u8], _: bool) -> crate::auth::StoreFuture<'a, ()> {
        Box::pin(std::future::pending())
    }
}

#[tokio::test]
async fn timed_out_secure_write_leaves_the_old_generation_revoked() {
    let fixture: Fixture = Fixture::new().await;
    let provider: Provider = login(&fixture).await;
    let record: crate::auth::credentials::Record = provider
        .vault
        .read(&fixture.endpoint, &operation())
        .await
        .unwrap()
        .unwrap();
    let vault: Vault = Vault::isolated(
        std::sync::Arc::new(PendingWrite(fixture.memory.clone())),
        fixture.root.path().to_path_buf(),
    );
    let short: Operation = Operation::new(
        Deadline::new(Duration::from_millis(30)),
        CancellationToken::new(),
    );
    let lock: std::fs::File = vault.lock(&fixture.endpoint, &short).await.unwrap();
    assert_eq!(
        vault
            .write(&fixture.endpoint, &record, false, &short)
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Timeout
    );
    drop(lock);
    assert!(
        provider
            .authorization(&fixture.endpoint, &operation())
            .await
            .is_err()
    );
    provider.logout(&operation()).await.unwrap();
}
