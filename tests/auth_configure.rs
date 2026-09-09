mod support;

use serde_json::{Value, json};
use std::{
    path::PathBuf,
    process::{Command, Output, Stdio},
};
use support::Sandbox;

fn fixture() -> Sandbox {
    let sandbox: Sandbox = Sandbox::new();
    let path: PathBuf = path(&sandbox);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, json!({"schema_version": 1, "servers": {"s": {"definition": {
        "kind": "remote", "url": "https://example.com/mcp", "transport": "streamable_http", "authentication": "oauth"
    }}}}).to_string()).unwrap();
    sandbox
}

fn path(sandbox: &Sandbox) -> PathBuf {
    sandbox
        .root
        .path()
        .join("XDG_CONFIG_HOME/ripmcp/config.json")
}

fn definition(sandbox: &Sandbox) -> Value {
    let value: Value = ripmcp::json::parse(&std::fs::read(path(sandbox)).unwrap()).unwrap();
    value["servers"]["s"]["definition"].clone()
}

fn with_token(sandbox: &Sandbox, args: &[&str], token: &str) -> Output {
    let mut command: Command = Command::new(env!("CARGO_BIN_EXE_ripmcp"));
    command
        .env_clear()
        .current_dir(sandbox.root.path())
        .stdin(Stdio::null())
        .args(args);
    for key in [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "XDG_RUNTIME_DIR",
    ] {
        command.env(key, sandbox.root.path().join(key));
    }
    command.env("MY_TOKEN", token).output().unwrap()
}

#[test]
fn bearer_env_configures_without_reading_credentials_and_status_resolves_at_runtime() {
    let sandbox: Sandbox = fixture();
    let output: Output = sandbox.run(&["auth", "configure", "s", "--bearer-env", "MY_TOKEN"]);
    assert!(output.status.success(), "{output:?}");
    let configured: Value = definition(&sandbox);
    assert_eq!(configured["authentication"], "bearer");
    assert_eq!(configured["bearer"], json!({"env": "MY_TOKEN"}));
    assert!(configured.get("oauth_client").is_none());
    for token in ["first-token", "rotated-token"] {
        let output: Output = with_token(&sandbox, &["auth", "status", "s"], token);
        assert!(output.status.success(), "{output:?}");
        let report: Value = ripmcp::json::parse(&output.stdout).unwrap();
        assert_eq!(report["auth"]["state"], "available");
        assert_eq!(report["auth"]["remote_validity_checked"], false);
        assert!(!String::from_utf8_lossy(&output.stdout).contains(token));
        assert!(!String::from_utf8_lossy(&std::fs::read(path(&sandbox)).unwrap()).contains(token));
    }
    let output: Output = with_token(&sandbox, &["auth", "logout", "s"], "secret-token");
    assert_eq!(output.status.code(), Some(10));
    assert_eq!(definition(&sandbox), configured);
    let output: Output = sandbox.run(&["auth", "login", "s"]);
    assert_eq!(output.status.code(), Some(10));
    assert!(String::from_utf8_lossy(&output.stderr).contains("auth configure"));
}

#[test]
fn missing_or_invalid_bearer_credentials_fail_without_disclosure() {
    let sandbox: Sandbox = fixture();
    assert!(
        sandbox
            .run(&["auth", "configure", "s", "--bearer-env", "MY_TOKEN"])
            .status
            .success()
    );
    for token in ["", "Bearer secret", "secret\r\nInjected: true"] {
        let output: Output = with_token(&sandbox, &["auth", "status", "s"], token);
        assert_eq!(output.status.code(), Some(6));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("Injected"));
    }
    assert_eq!(sandbox.run(&["auth", "status", "s"]).status.code(), Some(6));
}

#[test]
fn custom_header_env_and_oauth_client_setup_are_offline_and_do_not_resolve_secrets() {
    let sandbox: Sandbox = fixture();
    let output: Output = sandbox.run(&[
        "auth",
        "configure",
        "s",
        "--header",
        "X-API-Key",
        "--header-env",
        "API_KEY",
    ]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        definition(&sandbox)["headers"]["X-API-Key"],
        json!({"env": "API_KEY"})
    );
    let output: Output = sandbox.run(&[
        "auth",
        "configure",
        "s",
        "--oauth-client-id",
        "my-app",
        "--issuer",
        "https://issuer.example",
        "--client-secret-env",
        "MY_APP_SECRET",
        "--scope",
        "repo",
        "--token-endpoint-auth-method",
        "client_secret_basic",
    ]);
    assert!(output.status.success(), "{output:?}");
    let client: Value = definition(&sandbox)["oauth_client"].clone();
    assert_eq!(client["client_id"], "my-app");
    assert_eq!(client["client_secret"], json!({"env": "MY_APP_SECRET"}));
    assert_eq!(client["token_endpoint_auth_method"], "client_secret_basic");
    assert_eq!(client["scopes"], json!(["repo"]));
    let output: Output = sandbox.run(&["auth", "status", "s"]);
    assert_eq!(output.status.code(), Some(6));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Secret Service"));
}

#[test]
fn invalid_setup_and_unavailable_keyring_leave_configuration_unchanged() {
    let sandbox: Sandbox = fixture();
    let before: Vec<u8> = std::fs::read(path(&sandbox)).unwrap();
    for flags in [
        vec!["--bearer-env", "INVALID-NAME"],
        vec!["--header", "Host", "--header-env", "TOKEN"],
        vec!["--header", "Authorization", "--header-env", "TOKEN"],
        vec![
            "--oauth-client-id",
            "my-app",
            "--issuer",
            "http://issuer.example",
        ],
        vec![
            "--oauth-client-id",
            "my-app",
            "--issuer",
            "https://issuer.example",
            "--token-endpoint-auth-method",
            "client_secret_post",
        ],
        vec!["--bearer"],
    ] {
        let mut args: Vec<&str> = vec!["auth", "configure", "s"];
        args.extend(flags);
        let output: Output = sandbox.run(&args);
        assert!(!output.status.success(), "{args:?}");
        assert!(output.stdout.is_empty());
        assert_eq!(std::fs::read(path(&sandbox)).unwrap(), before);
    }
}

#[test]
fn local_targets_and_untrusted_project_overrides_are_not_mutated() {
    let sandbox: Sandbox = fixture();
    let project: PathBuf = sandbox.root.path().join(".ripmcp");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(
        project.join("config.json"),
        json!({"schema_version": 1, "servers": {"s": {"definition": {
            "kind": "local", "runtime": "npx", "package": "fixture", "transport": "stdio"
        }}}})
        .to_string(),
    )
    .unwrap();
    let before: Vec<u8> = std::fs::read(path(&sandbox)).unwrap();
    for scope in ["--user", "--project"] {
        let output: Output =
            sandbox.run(&["auth", "configure", "s", "--bearer-env", "TOKEN", scope]);
        assert_eq!(output.status.code(), Some(3));
        assert_eq!(std::fs::read(path(&sandbox)).unwrap(), before);
    }
    std::fs::copy(project.join("config.json"), path(&sandbox)).unwrap();
    std::fs::remove_dir_all(project).unwrap();
    let output: Output = sandbox.run(&["auth", "configure", "s", "--bearer-env", "TOKEN"]);
    assert_eq!(output.status.code(), Some(10));
}
