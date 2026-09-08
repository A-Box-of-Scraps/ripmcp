use super::support::{Fixture, TestBrowser, callback, operation};
use crate::auth::{Provider, metadata::Profile};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[tokio::test]
async fn explicit_resource_metadata_url_takes_precedence() {
    let fixture: Fixture = Fixture::new().await;
    {
        let mut settings: std::sync::MutexGuard<'_, super::support::Settings> =
            fixture.settings.lock().unwrap();
        let body: String = settings.resource.to_string();
        settings
            .replies
            .insert("/metadata".to_owned(), ("200 OK".to_owned(), body));
        settings.challenge = Some(format!(
            "Bearer resource_metadata=\"{}\"",
            fixture.endpoint.join("metadata").unwrap()
        ));
    }
    fixture
        .provider()
        .network
        .discover(&fixture.endpoint, &operation())
        .await
        .unwrap();
    assert_eq!(fixture.count("/metadata"), 1);
    assert_eq!(
        fixture.count("/.well-known/oauth-protected-resource/mcp"),
        0
    );
}

#[tokio::test]
async fn path_issuer_attempts_all_three_discovery_locations_in_order() {
    let fixture: Fixture = Fixture::new().await;
    {
        let mut settings: std::sync::MutexGuard<'_, super::support::Settings> =
            fixture.settings.lock().unwrap();
        let issuer: String = format!(
            "{}/tenant",
            settings.authorization["issuer"].as_str().unwrap()
        );
        settings.authorization["issuer"] = json!(issuer);
        settings.resource["authorization_servers"] = json!([issuer]);
        let body: String = settings.authorization.to_string();
        settings.replies.insert(
            "/tenant/.well-known/openid-configuration".to_owned(),
            ("200 OK".to_owned(), body),
        );
    }
    fixture
        .provider()
        .network
        .discover(&fixture.endpoint, &operation())
        .await
        .unwrap();
    let settings: std::sync::MutexGuard<'_, super::support::Settings> =
        fixture.settings.lock().unwrap();
    let paths: Vec<&str> = settings
        .requests
        .iter()
        .skip(2)
        .map(|request| request.split_whitespace().nth(1).unwrap())
        .collect();
    assert_eq!(
        paths,
        [
            "/.well-known/oauth-authorization-server/tenant",
            "/.well-known/openid-configuration/tenant",
            "/tenant/.well-known/openid-configuration"
        ]
    );
}

#[tokio::test]
async fn issuer_comparison_does_not_normalize_trailing_slashes() {
    let fixture: Fixture = Fixture::new().await;
    let issuer: String = format!("{}/", fixture.issuer());
    fixture.settings.lock().unwrap().authorization["issuer"] = json!(issuer);
    assert!(
        fixture
            .provider()
            .network
            .discover(&fixture.endpoint, &operation())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn redirects_and_duplicate_metadata_fail_closed() {
    for (status, body) in [
        ("302 Found", "{}"),
        ("200 OK", "{\"resource\":\"a\",\"resource\":\"b\"}"),
    ] {
        let fixture: Fixture = Fixture::new().await;
        fixture.settings.lock().unwrap().replies.insert(
            "/.well-known/oauth-protected-resource/mcp".to_owned(),
            (status.to_owned(), body.to_owned()),
        );
        assert!(
            fixture
                .provider()
                .network
                .discover(&fixture.endpoint, &operation())
                .await
                .is_err()
        );
        assert_eq!(fixture.count("/.well-known/oauth-protected-resource"), 0);
    }
}

#[tokio::test]
async fn native_dcr_fallback_validates_registration_response() {
    let fixture: Fixture = Fixture::new().await;
    {
        let mut settings: std::sync::MutexGuard<'_, super::support::Settings> =
            fixture.settings.lock().unwrap();
        settings.authorization["client_id_metadata_document_supported"] = json!(false);
        settings.authorization["registration_endpoint"] =
            json!(fixture.endpoint.join("register").unwrap().as_str());
        let body: Value = json!({"client_id": "registered-client", "token_endpoint_auth_method": "none", "redirect_uris": [crate::auth::REDIRECT_URI]});
        settings.replies.insert(
            "/register".to_owned(),
            ("201 Created".to_owned(), body.to_string()),
        );
    }
    let provider: Provider = fixture.provider();
    let profile: Profile = provider
        .network
        .discover(&fixture.endpoint, &operation())
        .await
        .unwrap();
    assert_eq!(
        provider
            .network
            .register(&profile, None, &operation())
            .await
            .unwrap(),
        "registered-client"
    );
    let settings: std::sync::MutexGuard<'_, super::support::Settings> =
        fixture.settings.lock().unwrap();
    let request: &String = settings
        .requests
        .iter()
        .find(|request| request.starts_with("POST /register "))
        .unwrap();
    let body: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["application_type"], "native");
    assert_eq!(body["token_endpoint_auth_method"], "none");
}

#[tokio::test]
async fn confidential_or_redirect_substituting_dcr_responses_are_rejected() {
    for field in [
        "client_secret",
        "token_endpoint_auth_method",
        "redirect_uris",
    ] {
        let fixture: Fixture = Fixture::new().await;
        let provider: Provider = fixture.provider();
        let mut profile: Profile = provider
            .network
            .discover(&fixture.endpoint, &operation())
            .await
            .unwrap();
        profile.authorization.client_id_metadata_document_supported = Some(false);
        profile.authorization.registration_endpoint =
            Some(fixture.endpoint.join("register").unwrap().to_string());
        let mut body: Value = json!({"client_id": "client", "token_endpoint_auth_method": "none", "redirect_uris": [crate::auth::REDIRECT_URI]});
        body[field] = json!("secret-invalid");
        fixture.settings.lock().unwrap().replies.insert(
            "/register".to_owned(),
            ("201 Created".to_owned(), body.to_string()),
        );
        assert!(
            provider
                .network
                .register(&profile, None, &operation())
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn invalid_token_responses_never_persist_active_credentials() {
    for (field, value) in [
        ("token_type", json!("MAC")),
        ("access_token", json!("token\r\ninjected")),
        ("expires_in", json!(0)),
        ("expires_in", json!(-1)),
        ("expires_in", json!(u64::MAX)),
        ("refresh_token", json!("")),
    ] {
        let fixture: Fixture = Fixture::new().await;
        fixture.settings.lock().unwrap().token[field] = value;
        let provider: Provider = fixture.provider();
        assert!(
            provider
                .login_with(
                    callback().await,
                    &TestBrowser::new(fixture.issuer()),
                    || Ok(()),
                    &operation()
                )
                .await
                .is_err()
        );
        assert_eq!(provider.status(&operation()).await.unwrap(), "signed_out");
    }
}

#[tokio::test]
async fn callback_scope_is_not_limited_to_advertised_resource_scopes() {
    let fixture: Fixture = Fixture::new().await;
    fixture.settings.lock().unwrap().challenge =
        Some("Bearer scope=\"other:permission\"".to_owned());
    let browser: TestBrowser = TestBrowser::new(fixture.issuer());
    fixture
        .provider()
        .login_with(callback().await, &browser, || Ok(()), &operation())
        .await
        .unwrap();
    let query: BTreeMap<String, String> = browser.urls.lock().unwrap()[0]
        .query_pairs()
        .into_owned()
        .collect();
    assert_eq!(query["scope"], "other:permission");
}

#[test]
fn private_networks_and_transition_addresses_are_not_oauth_destinations() {
    for address in [
        "127.0.0.1",
        "10.0.0.1",
        "169.254.169.254",
        "192.168.1.1",
        "100.64.0.1",
        "0.0.0.0",
        "224.0.0.1",
        "::1",
        "::ffff:127.0.0.1",
        "fc00::1",
        "fe80::1",
        "2002:7f00:1::",
        "64:ff9b::7f00:1",
    ] {
        assert!(!crate::auth::destination::public(address.parse().unwrap()));
    }
    assert!(crate::auth::destination::public("8.8.8.8".parse().unwrap()));
    assert!(crate::auth::destination::public(
        "2606:4700:4700::1111".parse().unwrap()
    ));
}

#[tokio::test]
async fn challenge_cannot_redirect_metadata_fetch_to_instance_credentials() {
    let fixture: Fixture = Fixture::new().await;
    fixture.settings.lock().unwrap().challenge =
        Some("Bearer resource_metadata=\"http://169.254.169.254/latest/meta-data/\"".to_owned());
    assert!(
        fixture
            .provider()
            .network
            .discover(&fixture.endpoint, &operation())
            .await
            .is_err()
    );
    assert_eq!(fixture.settings.lock().unwrap().requests.len(), 1);
}

#[tokio::test]
async fn oversized_metadata_is_rejected_without_partial_results() {
    let fixture: Fixture = Fixture::new().await;
    let body: String = json!({"padding": "x".repeat(crate::auth::network::BODY_LIMIT)}).to_string();
    fixture.settings.lock().unwrap().replies.insert(
        "/.well-known/oauth-protected-resource/mcp".to_owned(),
        ("200 OK".to_owned(), body),
    );
    assert!(
        fixture
            .provider()
            .network
            .discover(&fixture.endpoint, &operation())
            .await
            .is_err()
    );
}
