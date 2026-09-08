#[path = "config/ownership.rs"]
mod ownership;
#[path = "config/storage.rs"]
mod storage;
#[path = "config/terminal.rs"]
mod terminal;
#[path = "config/trust.rs"]
mod trust;

use ripmcp::config::{Configuration, Effective, Project, WriteScope};
use ripmcp::storage::{Location, Paths};
use ripmcp::trust::TrustStore;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

struct Fixture {
    root: TempDir,
    paths: Paths,
}
impl Fixture {
    fn new() -> Self {
        let root: TempDir = tempfile::tempdir().unwrap();
        private(root.path(), 0o700);
        let paths: Paths = Paths::new(BTreeMap::from([(
            OsString::from("HOME"),
            root.path().as_os_str().to_owned(),
        )]));
        Self { root, paths }
    }
    fn project(&self, relative: &str, config: &Value) -> PathBuf {
        let root: PathBuf = self.root.path().join(relative);
        write(
            &root.join(".ripmcp/config.json"),
            &serde_json::to_vec(config).unwrap(),
        );
        root
    }
    fn user(&self, config: &Value) {
        write(
            &self
                .paths
                .directory(Location::Config)
                .unwrap()
                .join("config.json"),
            &serde_json::to_vec(config).unwrap(),
        );
    }
    fn load(&self, cwd: &Path) -> Effective {
        Effective::load(&self.paths, cwd).unwrap()
    }
    fn approve(&self, cwd: &Path) {
        let project: Project = Project::discover(cwd).unwrap().unwrap();
        TrustStore::new(&self.paths)
            .unwrap()
            .approve(&project)
            .unwrap();
    }
    fn run(&self, cwd: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ripmcp"))
            .env_clear()
            .env("HOME", self.root.path())
            .current_dir(cwd)
            .stdin(Stdio::null())
            .args(args)
            .output()
            .unwrap()
    }
}

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}
fn private(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
fn local(package: &str) -> Value {
    json!({"definition":{"kind":"local","runtime":"npx","package":package,"transport":"stdio"}})
}
fn remote(url: &str) -> Value {
    json!({"definition":{"kind":"remote","url":url,"transport":"streamable_http"}})
}
fn config(server: Value) -> Value {
    json!({"schema_version":1,"servers":{"s":server}})
}

#[test]
fn nearest_project_replaces_whole_definition_and_settings() {
    let fixture: Fixture = Fixture::new();
    fixture.user(&json!({"schema_version":1,"servers":{"s":local("global"),"user-only":local("other")},"timeouts":{"operation_seconds":70,"login_seconds":400}}));
    let outer: PathBuf = fixture.project("outer", &config(local("outer")));
    let inner: PathBuf = fixture.project("outer/inner", &json!({"schema_version":1,"servers":{"s":remote("https://example.test/mcp")},"timeouts":{"operation_seconds":3}}));
    let nested: PathBuf = inner.join("a/b/c");
    fs::create_dir_all(&nested).unwrap();
    let effective: Effective = fixture.load(&nested);
    assert_eq!(effective.reports().len(), 2);
    assert!(effective.authorize("s").is_err());
    assert_eq!(effective.timeouts().operation_seconds.get(), 70);
    assert_eq!(Project::discover(&nested).unwrap().unwrap().root(), inner);
    fixture.approve(&nested);
    let effective: Effective = fixture.load(&nested);
    assert!(matches!(
        effective.authorize("s").unwrap().server().definition,
        ripmcp::config::Definition::Remote { .. }
    ));
    assert_eq!(effective.timeouts().operation_seconds.get(), 3);
    assert_eq!(effective.timeouts().login_seconds.get(), 300);
    assert_eq!(Project::discover(&outer).unwrap().unwrap().root(), outer);
}

#[test]
fn missing_config_continues_but_malformed_selected_config_never_falls_back() {
    let fixture: Fixture = Fixture::new();
    let outer: PathBuf = fixture.project("outer", &config(local("outer")));
    let child: PathBuf = outer.join("child");
    fs::create_dir_all(child.join(".ripmcp")).unwrap();
    assert_eq!(Project::discover(&child).unwrap().unwrap().root(), outer);
    write(&child.join(".ripmcp/config.json"), b"{bad");
    assert!(Project::discover(&child).is_err());
    fixture.user(&config(local("global")));
    assert!(Effective::load(&fixture.paths, &child).is_err());
}

#[test]
fn schema_versions_fields_transports_and_secrets_fail_closed() {
    let invalid: Vec<Value> = vec![
        json!({}),
        json!({"schema_version":2}),
        json!({"schema_version":1,"unknown":1}),
        json!({"schema_version":1,"timeouts":null}),
        json!({"schema_version":1,"timeouts":{"operation_seconds":0}}),
        config(
            json!({"definition":{"kind":"local","runtime":"node","package":"pkg","transport":"stdio"}}),
        ),
        config(
            json!({"definition":{"kind":"local","runtime":"npx","package":"pkg","transport":"streamable_http"}}),
        ),
        config(
            json!({"definition":{"kind":"remote","url":"https://example.test","transport":"stdio"}}),
        ),
        config(remote("https://user:secret@example.test")),
        config(remote("file:///tmp/socket")),
        config(remote("https://example.test/#fragment")),
        config(
            json!({"definition":{"kind":"local","runtime":"npx","package":"pkg","transport":"stdio","env":{"TOKEN":"secret"}}}),
        ),
        config(
            json!({"definition":{"kind":"local","runtime":"npx","package":"pkg","transport":"stdio","env":{"TOKEN":{"env":"BAD-NAME"}}}}),
        ),
        config(
            json!({"definition":{"kind":"local","runtime":"npx","package":"pkg","transport":"stdio","env":{"TOKEN":{"env":"NAME","keyring":"id"}}}}),
        ),
        config(
            json!({"definition":{"kind":"remote","url":"https://example.test","transport":"streamable_http","headers":{"Authorization":{"keyring":"/secret/path"}}}}),
        ),
        config(
            json!({"definition":{"kind":"remote","url":"https://example.test","transport":"streamable_http","headers":{"Authorization":{"keyring":""}}}}),
        ),
    ];
    for value in invalid {
        let bytes: Vec<u8> = serde_json::to_vec(&value).unwrap();
        let error: ripmcp::error::Error = Configuration::parse(&bytes).err().unwrap();
        assert!(!error.to_string().contains("user:secret"));
        assert!(!error.to_string().contains("/secret/path"));
        assert_eq!(error.kind, ripmcp::error::ErrorKind::Configuration);
    }
    assert!(Configuration::parse(br#"{"schema_version":1,"servers":{},"servers":{}}"#).is_err());
    assert!(Configuration::parse(br#"{"schema_version":1} {}"#).is_err());
    let duplicate: String = format!(
        r#"{{"schema_version":1,"servers":{{"s":{},"s":{}}}}}"#,
        local("a"),
        local("b")
    );
    assert!(Configuration::parse(duplicate.as_bytes()).is_err());
}

#[test]
fn explicit_scope_writes_report_shadows_and_invalidate_approval() {
    let fixture: Fixture = Fixture::new();
    fixture.user(&config(local("user")));
    let root: PathBuf = fixture.project("project", &config(local("project")));
    fixture.approve(&root);
    let effective: Effective = fixture.load(&root);
    let (_, report): ((), ripmcp::config::MutationReport) = effective
        .mutate(&fixture.paths, WriteScope::User, "s", |config| {
            config.servers.get_mut("s").unwrap().enabled = false;
            Ok(())
        })
        .unwrap();
    assert!(report.shadowed_by_trusted_project);
    assert!(!report.project_reapproval_required);
    assert!(fixture.load(&root).authorize("s").unwrap().server().enabled);
    let (_, report): ((), ripmcp::config::MutationReport) = effective
        .mutate(&fixture.paths, WriteScope::Project, "s", |config| {
            config
                .servers
                .get_mut("s")
                .unwrap()
                .disabled_tools
                .insert("tool".to_owned());
            Ok(())
        })
        .unwrap();
    assert!(report.project_reapproval_required);
    assert!(fixture.load(&root).authorize("s").is_err());
    assert!(
        effective
            .mutate(&fixture.paths, WriteScope::Project, "s", |_| Ok(()))
            .is_err()
    );
    let outside: Effective = fixture.load(fixture.root.path());
    assert!(
        outside
            .mutate(&fixture.paths, WriteScope::Project, "s", |_| Ok(()))
            .is_err()
    );
}

#[test]
fn duplicate_registration_does_not_overwrite_and_policy_is_default_allow() {
    let mut configuration: Configuration =
        Configuration::parse(&serde_json::to_vec(&config(local("pkg"))).unwrap()).unwrap();
    let server: ripmcp::config::Server = configuration.servers["s"].clone();
    assert!(configuration.register("s".to_owned(), server).is_err());
    assert!(configuration.servers["s"].enabled);
    assert!(configuration.servers["s"].disabled_tools.is_empty());
    let fixture: Fixture = Fixture::new();
    fixture.user(&config(local("pkg")));
    let effective: Effective = fixture.load(fixture.root.path());
    assert!(
        effective
            .authorize("s")
            .unwrap()
            .require_enabled(Some("new-tool"))
            .is_ok()
    );
}

#[test]
fn passive_cli_is_redacted_and_does_not_create_storage() {
    let fixture: Fixture = Fixture::new();
    let root: PathBuf = fixture.project(
        "project",
        &config(remote("https://example.test/secret-path?token=secret")),
    );
    let output: Output = fixture.run(&root, &["servers"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["servers"][0]["trust_required"], true);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("secret"));
    assert!(!fixture.root.path().join(".local").exists());
    assert!(!fixture.root.path().join(".config").exists());
    let output: Output = fixture.run(&root, &["trust"]);
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("terminal"));
    assert!(!fixture.root.path().join(".local").exists());
}

#[test]
fn empty_servers_without_home_errors_but_xdg_only_read_is_lazy() {
    let fixture: Fixture = Fixture::new();
    let paths: Paths = Paths::new(BTreeMap::new());
    assert!(paths.directory(Location::Config).is_err());
    let paths: Paths = Paths::new(BTreeMap::from([(
        OsString::from("XDG_CONFIG_HOME"),
        fixture.root.path().join("config").into_os_string(),
    )]));
    assert!(
        Effective::load(&paths, fixture.root.path())
            .unwrap()
            .reports()
            .is_empty()
    );
    assert_eq!(fs::read_dir(fixture.root.path()).unwrap().count(), 0);
}

#[test]
fn stale_scope_selection_is_rejected_and_unchanged_bytes_keep_approval() {
    let fixture: Fixture = Fixture::new();
    let root: PathBuf = fixture.project("project", &config(local("pkg")));
    let cwd: PathBuf = root.join("nested");
    fs::create_dir(&cwd).unwrap();
    fixture.approve(&root);
    fixture
        .load(&cwd)
        .mutate(&fixture.paths, WriteScope::Project, "s", |_| Ok(()))
        .unwrap();
    fixture.approve(&root);
    let effective: Effective = fixture.load(&cwd);
    let (_, report): ((), ripmcp::config::MutationReport) = effective
        .mutate(&fixture.paths, WriteScope::Project, "s", |_| Ok(()))
        .unwrap();
    assert!(!report.project_reapproval_required);
    assert!(fixture.load(&root).authorize("s").is_ok());
    fixture.project("project/nested", &config(local("new-project")));
    assert!(
        effective
            .mutate(&fixture.paths, WriteScope::Project, "s", |_| Ok(()))
            .is_err()
    );
}
