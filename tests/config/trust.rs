use super::*;
use ripmcp::config::AuthorizedServer;
use ripmcp::error::{Error, ErrorKind};
use ripmcp::trust::{
    Preview,
    secrets::{Secret, SecretBackend},
};
use std::cell::Cell;
use std::os::unix::fs::symlink;

#[test]
fn exact_bytes_endpoint_changes_moves_and_symlink_aliases_bind_trust() {
    let fixture: Fixture = Fixture::new();
    let root: PathBuf = fixture.project("project", &config(remote("https://one.test/mcp")));
    fixture.approve(&root);
    assert!(fixture.load(&root).authorize("s").is_ok());
    let alias: PathBuf = fixture.root.path().join("alias");
    symlink(&root, &alias).unwrap();
    assert!(fixture.load(&alias).authorize("s").is_ok());
    assert_eq!(Project::discover(&alias).unwrap().unwrap().root(), root);
    let bytes: Vec<u8> = fs::read(root.join(".ripmcp/config.json")).unwrap();
    let mut changed: Vec<u8> = bytes.clone();
    changed.push(b'\n');
    write(&root.join(".ripmcp/config.json"), &changed);
    assert!(fixture.load(&root).authorize("s").is_err());
    fixture.approve(&root);
    fixture.project("project", &config(remote("https://two.test/mcp")));
    assert!(fixture.load(&root).authorize("s").is_err());
    fixture.approve(&root);
    let moved: PathBuf = fixture.root.path().join("moved");
    fs::rename(&root, &moved).unwrap();
    assert!(fixture.load(&moved).authorize("s").is_err());
    fs::remove_file(&alias).unwrap();
    symlink(&moved, &alias).unwrap();
    assert!(fixture.load(&alias).authorize("s").is_err());
}

#[test]
fn approval_is_bound_to_previewed_bytes_not_a_reread() {
    let fixture: Fixture = Fixture::new();
    let root: PathBuf = fixture.project("project", &config(local("safe")));
    let snapshot: Project = Project::discover(&root).unwrap().unwrap();
    let _: Preview<'_> = Preview::new(&snapshot).unwrap();
    fixture.project("project", &config(local("changed-after-preview")));
    let store: TrustStore = TrustStore::new(&fixture.paths).unwrap();
    store.approve(&snapshot).unwrap();
    assert!(store.is_approved(&snapshot).unwrap());
    assert!(fixture.load(&root).authorize("s").is_err());
}

#[test]
fn project_config_symlinks_cannot_borrow_approval() {
    let fixture: Fixture = Fixture::new();
    let root: PathBuf = fixture.project("project", &config(local("pkg")));
    fixture.approve(&root);
    let config_path: PathBuf = root.join(".ripmcp/config.json");
    let outside: PathBuf = fixture.root.path().join("outside.json");
    fs::rename(&config_path, &outside).unwrap();
    symlink(&outside, &config_path).unwrap();
    assert!(Project::discover(&root).is_err());
    fs::remove_file(&config_path).unwrap();
    fs::remove_dir(root.join(".ripmcp")).unwrap();
    symlink(fixture.root.path(), root.join(".ripmcp")).unwrap();
    assert!(Project::discover(&root).is_err());
}

#[test]
fn corrupt_incompatible_or_public_trust_state_is_not_accepted_or_overwritten() {
    let fixture: Fixture = Fixture::new();
    let root: PathBuf = fixture.project("project", &config(local("pkg")));
    fixture.approve(&root);
    let state: PathBuf = fixture
        .paths
        .directory(Location::State)
        .unwrap()
        .join("trust.json");
    let project: Project = Project::discover(&root).unwrap().unwrap();
    for bytes in [
        b"{".as_slice(),
        br#"{"schema_version":2,"approvals":{}}"#,
        br#"{"approvals":{}}"#,
    ] {
        fs::write(&state, bytes).unwrap();
        assert!(fixture.load_result(&root).is_err());
        assert!(
            TrustStore::new(&fixture.paths)
                .unwrap()
                .approve(&project)
                .is_err()
        );
        assert_eq!(fs::read(&state).unwrap(), bytes);
    }
    private(&state, 0o644);
    assert!(
        TrustStore::new(&fixture.paths)
            .unwrap()
            .is_approved(&project)
            .is_err()
    );
}

impl Fixture {
    fn load_result(&self, cwd: &Path) -> Result<Effective, Error> {
        Effective::load(&self.paths, cwd)
    }
}

struct CountingBackend(Cell<u32>);
impl SecretBackend for CountingBackend {
    fn environment(&self, _: &str) -> Result<Secret, Error> {
        self.0.set(self.0.get() + 1);
        Ok(Secret::new("secret-value".to_owned()))
    }
    fn keyring(&self, _: &str) -> Result<Secret, Error> {
        self.0.set(self.0.get() + 1);
        Err(Error::new(ErrorKind::Authentication, "not available"))
    }
}

#[test]
fn secret_access_requires_authorization_and_disabled_policy_wins() {
    let fixture: Fixture = Fixture::new();
    fixture.user(&config(local("global")));
    let mut server: Value = local("project");
    server["definition"]["env"] = json!({"TOKEN":{"env":"TOKEN_SOURCE"}});
    let root: PathBuf = fixture.project("project", &config(server.clone()));
    let backend: CountingBackend = CountingBackend(Cell::new(0));
    let effective: Effective = fixture.load(&root);
    assert!(effective.authorize("s").is_err());
    assert_eq!(backend.0.get(), 0);
    fixture.approve(&root);
    let effective: Effective = fixture.load(&root);
    let authorized: AuthorizedServer<'_> = effective.authorize("s").unwrap();
    let secrets: BTreeMap<String, Secret> = authorized.resolve_secrets(&backend).unwrap();
    assert_eq!(backend.0.get(), 1);
    assert_eq!(secrets["TOKEN"].expose(), "secret-value");
    assert!(!format!("{secrets:?}").contains("secret-value"));
    server["enabled"] = json!(false);
    fixture.project("project", &config(server));
    fixture.approve(&root);
    assert!(
        fixture
            .load(&root)
            .authorize("s")
            .unwrap()
            .resolve_secrets(&backend)
            .is_err()
    );
    assert_eq!(backend.0.get(), 1);
}

#[test]
fn preview_exposes_effects_but_not_arguments_references_or_url_secrets() {
    let fixture: Fixture = Fixture::new();
    let mut local: Value = local("pkg");
    local["definition"]["args"] = json!(["--token", "secret-arg"]);
    local["definition"]["env"] = json!({"TOKEN":{"keyring":"secret-reference"}});
    let root: PathBuf = fixture.project("project", &json!({"schema_version":1,"servers":{"local":local,"remote":remote("https://example.test/secret-path?token=secret-query")}}));
    let project: Project = Project::discover(&root).unwrap().unwrap();
    let preview: String = serde_json::to_string(&Preview::new(&project).unwrap()).unwrap();
    for secret in [
        "secret-arg",
        "secret-reference",
        "secret-path",
        "secret-query",
    ] {
        assert!(!preview.contains(secret));
    }
    assert!(preview.contains("https://example.test"));
    assert!(preview.contains("npx"));
    assert!(preview.contains("pkg"));
    assert!(preview.contains(project.digest()));
}

#[test]
fn repository_cannot_supply_its_own_approval_state() {
    let fixture: Fixture = Fixture::new();
    let root: PathBuf = fixture.project("project", &config(local("pkg")));
    let paths: Paths = Paths::new(BTreeMap::from([(
        OsString::from("XDG_STATE_HOME"),
        root.join("state").into_os_string(),
    )]));
    let project: Project = Project::discover(&root).unwrap().unwrap();
    let store: TrustStore = TrustStore::new(&paths).unwrap();
    assert!(store.approve(&project).is_err());
    assert!(store.is_approved(&project).is_err());
    assert!(!root.join("state").exists());
}

#[test]
fn malformed_approval_entries_are_preserved_instead_of_silently_repaired() {
    let fixture: Fixture = Fixture::new();
    let root: PathBuf = fixture.project("project", &config(local("pkg")));
    fixture.approve(&root);
    let path: PathBuf = fixture
        .paths
        .directory(Location::State)
        .unwrap()
        .join("trust.json");
    let bytes: &[u8] = br#"{"schema_version":1,"approvals":{"relative":"invalid-digest"}}"#;
    fs::write(&path, bytes).unwrap();
    let project: Project = Project::discover(&root).unwrap().unwrap();
    let store: TrustStore = TrustStore::new(&fixture.paths).unwrap();
    assert!(store.is_approved(&project).is_err());
    assert!(store.approve(&project).is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn authorized_snapshot_does_not_reread_changed_execution_bytes() {
    let fixture: Fixture = Fixture::new();
    let root: PathBuf = fixture.project("project", &config(local("approved-package")));
    fixture.approve(&root);
    let effective: Effective = fixture.load(&root);
    fixture.project("project", &config(local("unapproved-package")));
    let authorized: AuthorizedServer<'_> = effective.authorize("s").unwrap();
    let ripmcp::config::Definition::Local { package, .. }: &ripmcp::config::Definition =
        &authorized.server().definition
    else {
        panic!("expected local definition");
    };
    assert_eq!(package, "approved-package");
    assert!(fixture.load(&root).authorize("s").is_err());
}
