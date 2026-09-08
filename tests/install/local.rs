use super::fixture::Fixture;
use ripmcp::ownership::{Journal, Origin, OwnershipStore, Registration, Verification};
use serde_json::{Value, json};
use std::{fs, process::Output};

#[test]
fn all_runtimes_prepare_and_skip_does_not_launch() {
    for runtime in ["--npx", "--uvx", "--docker"] {
        let fixture: Fixture = Fixture::new();
        let report: Value = fixture.ok(&[
            "install",
            "s",
            runtime,
            "fixture",
            "--skip-verify",
            "--",
            "literal ; $HOME",
            "--flag=value",
        ]);
        assert_eq!(report["installation"]["verification"], "unverified");
        assert_eq!(report["installation"]["process_retained"], false);
        assert!(fixture.log().contains("fixture"));
        assert!(!fixture.log().contains("server/discover"));
        assert!(!fixture.root.path().join("XDG_DATA_HOME/ripmcp").exists());
        let journal: Journal = OwnershipStore::new(&fixture.paths).unwrap().read().unwrap();
        let entry: &ripmcp::ownership::Installation =
            journal.installations.values().next().unwrap();
        assert_eq!(entry.registration, Registration::Registered);
        assert_eq!(entry.verification, Verification::Unverified);
        let Origin::Local {
            resolved: Some(resolved),
            executable: Some(executable),
            ..
        }: &Origin = &entry.origin
        else {
            panic!("unprepared")
        };
        assert!(resolved.contains("1.2.3") || resolved.contains("@sha256:"));
        assert!(executable.is_absolute());
        let _: Value = fixture.ok(&["start", "s"]);
        assert!(fixture.log().contains("literal ; $HOME"));
        let _: Value = fixture.ok(&["stop", "s"]);
    }
}

#[test]
fn verification_discovers_without_calls_and_retains_process() {
    for runtime in ["--npx", "--uvx", "--docker"] {
        let fixture: Fixture = Fixture::new();
        let report: Value = fixture.ok(&["install", "s", runtime, "fixture"]);
        assert_eq!(report["installation"]["verification"], "verified");
        assert_eq!(report["installation"]["process_retained"], true);
        assert!(fixture.log().contains("server/discover"));
        assert!(fixture.log().contains("tools/list"));
        assert!(!fixture.log().contains("tools/call"));
        let report: Value = fixture.ok(&["start", "s"]);
        assert_eq!(report["actions"][0]["reused"], true);
        assert_eq!(
            fs::read_to_string(fixture.root.path().join("launches"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        let _: Value = fixture.ok(&["stop", "s"]);
    }
}

#[test]
fn failures_never_register_and_redact_runtime_output() {
    for runtime in ["--npx", "--uvx", "--docker"] {
        let fixture: Fixture = Fixture::new();
        fixture.flag("fail");
        let output: Output = fixture.run(&["install", "s", runtime, "fixture", "--skip-verify"]);
        assert_eq!(output.status.code(), Some(4), "{output:?}");
        assert!(!fixture.config().exists());
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("upstream-secret"));
        let journal: Journal = OwnershipStore::new(&fixture.paths).unwrap().read().unwrap();
        assert!(
            journal
                .installations
                .values()
                .all(|entry| entry.registration == Registration::Unregistered)
        );
        assert_eq!(journal.operations.len(), 1);
    }
}

#[test]
fn missing_runtimes_bad_versions_and_unhonored_pins_fail() {
    for (runtime, package, flag) in [
        ("--npx", "fixture", "bad-version"),
        ("--uvx", "fixture", "bad-version"),
        ("--npx", "fixture@9.8.7", ""),
        ("--uvx", "fixture==9.8.7", ""),
        (
            "--docker",
            "fixture@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "",
        ),
    ] {
        let fixture: Fixture = Fixture::new();
        if !flag.is_empty() {
            fixture.flag(flag);
        }
        let output: Output = fixture.run(&["install", "s", runtime, package, "--skip-verify"]);
        assert_eq!(output.status.code(), Some(3), "{output:?}");
        assert!(!fixture.config().exists());
    }
    for runtime in ["npx", "uvx", "docker"] {
        let fixture: Fixture = Fixture::new();
        fs::remove_file(fixture.root.path().join(runtime)).unwrap();
        let output: Output = fixture.run(&[
            "install",
            "s",
            &format!("--{runtime}"),
            "fixture",
            "--skip-verify",
        ]);
        assert_eq!(output.status.code(), Some(10));
        assert!(!fixture.config().exists());
    }
}

#[test]
fn native_import_is_strict_and_arguments_stay_vectors() {
    let fixture: Fixture = Fixture::new();
    let path: std::path::PathBuf = fixture.import(&json!({"definition": {"kind": "local", "runtime": "npx", "package": "fixture", "transport": "stdio", "args": ["; touch /tmp/not-executed", "a b", "$(whoami)"]}}));
    let _: Value = fixture.ok(&[
        "install",
        "s",
        "--config",
        path.to_str().unwrap(),
        "--skip-verify",
    ]);
    for bytes in [
        r#"{"definition":{},"definition":{}}"#,
        r#"{"schema_version":1,"servers":{}}"#,
        r#"{"definition":{"kind":"local","runtime":"npx","package":"fixture","transport":"stdio","env":{"TOKEN":"inline-secret"}}}"#,
        r#"{"definition":{},"unknown":true}"#,
    ] {
        fs::write(&path, bytes).unwrap();
        let before: String = fixture.log();
        let output: Output = fixture.run(&[
            "install",
            "other",
            "--config",
            path.to_str().unwrap(),
            "--skip-verify",
        ]);
        assert_eq!(output.status.code(), Some(3));
        assert_eq!(fixture.log(), before);
    }
}

#[test]
#[ignore = "requires RIPMCP_SMOKE_RUNTIME and RIPMCP_SMOKE_PACKAGE; accesses real registries"]
fn real_runtime_smoke() {
    let mut fixture: Fixture = Fixture::new();
    let runtime: String = std::env::var("RIPMCP_SMOKE_RUNTIME").expect("set npx, uvx, or docker");
    assert!(["npx", "uvx", "docker"].contains(&runtime.as_str()));
    let package: String =
        std::env::var("RIPMCP_SMOKE_PACKAGE").expect("select a tools-capable MCP package/image");
    fixture
        .env
        .insert("PATH".into(), std::env::var_os("PATH").unwrap());
    let report: Value = fixture.ok(&[
        "install",
        "s",
        &format!("--{runtime}"),
        &package,
        "--timeout",
        "180",
    ]);
    assert_eq!(report["installation"]["verification"], "verified");
    let _: Value = fixture.ok(&["stop", "s"]);
}

#[test]
fn config_import_rejects_special_files_and_symlinks_without_blocking() {
    let fixture: Fixture = Fixture::new();
    let path: std::path::PathBuf = fixture.root.path().join("fifo");
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &path,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        0,
    )
    .unwrap();
    let output: Output = fixture.run(&[
        "install",
        "s",
        "--config",
        path.to_str().unwrap(),
        "--skip-verify",
    ]);
    assert_eq!(output.status.code(), Some(3));
    let link: std::path::PathBuf = fixture.root.path().join("link");
    std::os::unix::fs::symlink(path, &link).unwrap();
    let output: Output = fixture.run(&[
        "install",
        "s",
        "--config",
        link.to_str().unwrap(),
        "--skip-verify",
    ]);
    assert!(!output.status.success());
    assert!(fixture.log().is_empty());
}

#[test]
fn uvx_at_version_requests_use_an_exact_from_requirement() {
    for package in ["fixture@1.2.3", "fixture==1.2.3", "fixture@latest"] {
        let fixture: Fixture = Fixture::new();
        let _: Value = fixture.ok(&["install", "s", "--uvx", package]);
        assert!(
            fixture
                .log()
                .contains(r#"["--from", "fixture==1.2.3", "fixture"]"#)
        );
        let journal: Journal = OwnershipStore::new(&fixture.paths).unwrap().read().unwrap();
        let Origin::Local {
            requested,
            resolved,
            ..
        }: &Origin = &journal.installations.values().next().unwrap().origin
        else {
            panic!("not local")
        };
        assert_eq!(requested, package);
        assert_eq!(resolved.as_deref(), Some("fixture==1.2.3"));
    }
}

#[test]
fn local_secret_failures_do_not_suggest_remote_oauth() {
    let fixture: Fixture = Fixture::new();
    let path: std::path::PathBuf = fixture.import(&json!({"definition": {"kind":"local", "runtime":"npx", "package":"fixture", "transport":"stdio", "env":{"SECRET":{"env":"MISSING"}}}}));
    let output: Output = fixture.run(&["install", "s", "--config", path.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(6));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("auth login"));
    assert!(!fixture.config().exists());
}
