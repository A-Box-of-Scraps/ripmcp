use super::fixture::Fixture;
use ripmcp::{error::ErrorKind, storage::Store};
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[test]
fn full_stdout_reports_io_failure_without_replaying_the_installed_tool() {
    let fixture: Fixture = Fixture::new();
    let _: Value = fixture.ok(&["install", "s", "--npx", "fixture"]);
    let output: Output = fixture
        .command(&["call", "s", "echo", "{}"])
        .stdout(File::options().write(true).open("/dev/full").unwrap())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        fixture
            .log()
            .lines()
            .filter(|line| *line == "tools/call")
            .count(),
        1
    );
    assert_eq!(fixture.ok(&["start", "s"])["actions"][0]["reused"], true);
}

#[test]
#[ignore = "requires Linux unprivileged user/mount namespaces, unshare, sh and mount"]
fn disk_full_preserves_committed_config_and_recovers_after_space_is_released() {
    let mounted: Option<std::ffi::OsString> = std::env::var_os("RIPMCP_TEST_FULL_FILESYSTEM");
    if let Some(directory) = mounted {
        exercise_full_filesystem(Path::new(&directory));
        return;
    }
    let root: tempfile::TempDir = tempfile::tempdir().unwrap();
    let output: Output = Command::new("unshare")
        .args(["--user", "--map-root-user", "--mount", "sh", "-eu", "-c"])
        .arg("mount --make-rprivate /; mount -t tmpfs -o mode=1777 tmpfs /tmp; mkdir -p \"$RIPMCP_TEST_FULL_FILESYSTEM\"; mount -t tmpfs -o size=64k,mode=0700 tmpfs \"$RIPMCP_TEST_FULL_FILESYSTEM\"; exec \"$1\" --exact failures::disk_full_preserves_committed_config_and_recovers_after_space_is_released --ignored --nocapture")
        .arg("phase9")
        .arg(std::env::current_exe().unwrap())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("RIPMCP_TEST_FULL_FILESYSTEM", root.path())
        .output().unwrap();
    assert!(output.status.success(), "{output:?}");
}

fn exercise_full_filesystem(directory: &Path) {
    let store: Store = Store::new(directory.to_path_buf(), "probe.json", true).unwrap();
    store.update(|_| Ok((b"old".to_vec(), ()))).unwrap();
    assert_eq!(
        store
            .update(|_| Ok((vec![b'x'; 128 * 1024], ())))
            .unwrap_err()
            .kind,
        ErrorKind::Io
    );
    assert_eq!(store.read().unwrap().unwrap(), b"old");
    store.update(|_| Ok((b"new".to_vec(), ()))).unwrap();
    assert_eq!(store.read().unwrap().unwrap(), b"new");

    let mut fixture: Fixture = Fixture::new();
    fixture
        .env
        .insert("XDG_CONFIG_HOME".into(), directory.as_os_str().to_owned());
    fixture.paths = ripmcp::storage::Paths::new(fixture.env.clone());
    let import: PathBuf = fixture.import(&json!({"definition":{"kind":"remote", "url":"http://127.0.0.1:1/mcp", "transport":"streamable_http"}}));
    let _: Value = fixture.ok(&[
        "install",
        "s",
        "--config",
        import.to_str().unwrap(),
        "--skip-verify",
    ]);
    let config: PathBuf = directory.join("ripmcp/config.json");
    let before: Vec<u8> = fs::read(&config).unwrap();
    let filler: PathBuf = directory.join("filler");
    let mut file: File = File::create(&filler).unwrap();
    assert_eq!(
        file.write_all(&vec![0; 128 * 1024])
            .unwrap_err()
            .raw_os_error(),
        Some(28)
    );
    let output: Output = fixture.run(&["disable", "s"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read(&config).unwrap(), before);
    drop(file);
    fs::remove_file(filler).unwrap();
    let _: Value = fixture.ok(&["disable", "s"]);
    assert_eq!(fixture.ok(&["servers"])["servers"][0]["enabled"], false);
    assert!(fixture.log().is_empty());
}
