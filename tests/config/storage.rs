use super::*;
use ripmcp::error::{Error, ErrorKind};
use ripmcp::storage::{Directory, Store};
use std::os::unix::fs::{MetadataExt, symlink};
use std::sync::{Arc, Barrier};
use std::thread::{self, JoinHandle};

#[test]
fn xdg_defaults_absolute_overrides_and_relative_rejection_are_lazy() {
    let fixture: Fixture = Fixture::new();
    for (location, suffix) in [
        (Location::Config, ".config"),
        (Location::Data, ".local/share"),
        (Location::State, ".local/state"),
        (Location::Cache, ".cache"),
    ] {
        assert_eq!(
            fixture.paths.directory(location).unwrap(),
            fixture.root.path().join(suffix).join("ripmcp")
        );
    }
    let paths: Paths = Paths::new(BTreeMap::from([
        (
            OsString::from("HOME"),
            fixture.root.path().as_os_str().to_owned(),
        ),
        (
            OsString::from("XDG_CONFIG_HOME"),
            OsString::from("relative"),
        ),
        (
            OsString::from("XDG_DATA_HOME"),
            fixture.root.path().join("data").into_os_string(),
        ),
        (OsString::from("XDG_CACHE_HOME"), OsString::new()),
    ]));
    assert_eq!(
        paths.directory(Location::Config).unwrap(),
        fixture.paths.directory(Location::Config).unwrap()
    );
    assert_eq!(
        paths.directory(Location::Data).unwrap(),
        fixture.root.path().join("data/ripmcp")
    );
    assert_eq!(
        paths.directory(Location::Cache).unwrap(),
        fixture.paths.directory(Location::Cache).unwrap()
    );
    assert_eq!(fs::read_dir(fixture.root.path()).unwrap().count(), 0);
}

#[test]
fn runtime_fallback_creation_permissions_symlinks_and_wrong_uid() {
    let fixture: Fixture = Fixture::new();
    let uid = rustix::process::geteuid().as_raw();
    let fallback: PathBuf = fixture.root.path().join(format!("ripmcp-{uid}"));
    assert!(
        Paths::runtime_fallback(fixture.root.path(), uid, false)
            .unwrap()
            .is_none()
    );
    assert!(
        Paths::runtime_fallback(fixture.root.path(), uid, true)
            .unwrap()
            .is_some()
    );
    assert_eq!(fs::metadata(&fallback).unwrap().mode() & 0o777, 0o700);
    assert_eq!(fs::metadata(&fallback).unwrap().uid(), uid);
    assert!(Paths::runtime_fallback(fixture.root.path(), uid.wrapping_add(1), true).is_err());
    private(&fallback, 0o755);
    assert!(Paths::runtime_fallback(fixture.root.path(), uid, true).is_err());
    fs::remove_dir(&fallback).unwrap();
    symlink(fixture.root.path(), &fallback).unwrap();
    assert!(Paths::runtime_fallback(fixture.root.path(), uid, true).is_err());
    fs::remove_file(&fallback).unwrap();
    fs::write(&fallback, b"not a directory").unwrap();
    assert!(Paths::runtime_fallback(fixture.root.path(), uid, true).is_err());
}

#[test]
fn runtime_xdg_is_private_and_existing_unsafe_child_is_not_replaced() {
    let fixture: Fixture = Fixture::new();
    let paths: Paths = Paths::new(BTreeMap::from([(
        OsString::from("XDG_RUNTIME_DIR"),
        fixture.root.path().as_os_str().to_owned(),
    )]));
    assert!(paths.runtime(false).unwrap().is_none());
    assert!(paths.runtime(true).unwrap().is_some());
    let child: PathBuf = fixture.root.path().join("ripmcp");
    private(&child, 0o777);
    assert!(paths.runtime(true).is_err());
}

#[test]
fn secure_directory_rejects_foreign_owner_and_symlink_ancestors() {
    let fixture: Fixture = Fixture::new();
    symlink(fixture.root.path(), fixture.root.path().join("alias")).unwrap();
    assert!(Directory::open(&fixture.root.path().join("alias/child"), true, true).is_err());
    assert!(!fixture.root.path().join("child").exists());
    // /proc is root-owned, so an ordinary user cannot accept it as private state.
    assert!(Directory::open(Path::new("/proc"), false, true).is_err());
    assert!(Directory::open(Path::new("relative"), true, true).is_err());
    assert!(Directory::open(&fixture.root.path().join("../escape"), true, true).is_err());
}

#[test]
fn atomic_writes_preserve_old_bytes_on_failure_and_recover_pending_write() {
    let fixture: Fixture = Fixture::new();
    let directory: PathBuf = fixture.paths.directory(Location::State).unwrap();
    let store: Store = Store::new(directory.clone(), "state.json", true).unwrap();
    assert!(store.read().unwrap().is_none());
    store.update(|_| Ok((b"old".to_vec(), ()))).unwrap();
    let error: Result<(), Error> =
        store.update(|_| Err(Error::new(ErrorKind::Io, "injected failure")));
    assert!(error.is_err());
    assert_eq!(store.read().unwrap().unwrap(), b"old");
    write(
        &directory.join(".state.json.pending"),
        b"interrupted partial JSON",
    );
    assert_eq!(store.read().unwrap().unwrap(), b"old");
    store
        .update(|previous| {
            assert_eq!(previous.unwrap(), b"old");
            Ok((b"new".to_vec(), ()))
        })
        .unwrap();
    assert_eq!(store.read().unwrap().unwrap(), b"new");
    assert!(!directory.join(".state.json.pending").exists());
    assert_eq!(
        fs::metadata(directory.join("state.json")).unwrap().mode() & 0o777,
        0o600
    );
}

#[test]
fn concurrent_atomic_read_modify_write_does_not_lose_mutations() {
    let fixture: Fixture = Fixture::new();
    let barrier: Arc<Barrier> = Arc::new(Barrier::new(12));
    let mut workers: Vec<JoinHandle<()>> = Vec::new();
    for id in 0..12 {
        let barrier: Arc<Barrier> = barrier.clone();
        let directory: PathBuf = fixture.paths.directory(Location::State).unwrap();
        workers.push(thread::spawn(move || {
            let store: Store = Store::new(directory, "writers.json", true).unwrap();
            barrier.wait();
            store
                .update_json::<BTreeMap<String, u32>, _>(|values| {
                    values.insert(id.to_string(), id);
                    Ok(())
                })
                .unwrap();
        }));
    }
    for worker in workers {
        worker.join().unwrap();
    }
    let store: Store = Store::new(
        fixture.paths.directory(Location::State).unwrap(),
        "writers.json",
        true,
    )
    .unwrap();
    let values: BTreeMap<String, u32> =
        serde_json::from_slice(&store.read().unwrap().unwrap()).unwrap();
    assert_eq!(values.len(), 12);
}

#[test]
fn symlink_hardlink_fifo_and_unsafe_state_are_rejected_without_touching_target() {
    let fixture: Fixture = Fixture::new();
    let directory: PathBuf = fixture.root.path().join("state");
    fs::create_dir(&directory).unwrap();
    private(&directory, 0o700);
    let target: PathBuf = fixture.root.path().join("target");
    fs::write(&target, b"preserve").unwrap();
    private(&target, 0o600);
    let file: PathBuf = directory.join("state.json");
    let store: Store = Store::new(directory.clone(), "state.json", true).unwrap();
    symlink(&target, &file).unwrap();
    assert!(store.read().is_err());
    assert!(store.update(|_| Ok((Vec::new(), ()))).is_err());
    fs::remove_file(&file).unwrap();
    fs::hard_link(&target, &file).unwrap();
    assert!(store.read().is_err());
    fs::remove_file(&file).unwrap();
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &file,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        0,
    )
    .unwrap();
    assert!(store.read().is_err());
    fs::remove_file(&file).unwrap();
    fs::write(&file, b"unsafe mode").unwrap();
    private(&file, 0o644);
    assert!(store.read().is_err());
    fs::remove_file(&file).unwrap();
    fs::remove_file(directory.join(".state.json.lock")).unwrap();
    symlink(&target, directory.join(".state.json.lock")).unwrap();
    assert!(store.update(|_| Ok((Vec::new(), ()))).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"preserve");
}

#[test]
fn invalid_xdg_runtime_uses_only_the_validated_fallback() {
    let fixture: Fixture = Fixture::new();
    let xdg: PathBuf = fixture.root.path().join("xdg");
    fs::create_dir(&xdg).unwrap();
    private(&xdg, 0o755);
    let fallback: PathBuf = fixture.root.path().join("system-temp");
    fs::create_dir(&fallback).unwrap();
    let paths: Paths = Paths::new(BTreeMap::from([
        (
            OsString::from("XDG_RUNTIME_DIR"),
            xdg.clone().into_os_string(),
        ),
        (
            OsString::from("TMPDIR"),
            fixture.root.path().join("untrusted-temp").into_os_string(),
        ),
    ]));
    assert!(paths.runtime_at(&fallback, false).unwrap().is_none());
    assert!(paths.runtime_at(&fallback, true).unwrap().is_some());
    assert!(!xdg.join("ripmcp").exists());
    assert!(!fixture.root.path().join("untrusted-temp").exists());
    assert_eq!(fs::read_dir(&fallback).unwrap().count(), 1);
}

#[test]
fn concurrent_user_configuration_writers_preserve_each_registration() {
    let fixture: Fixture = Fixture::new();
    let barrier: Arc<Barrier> = Arc::new(Barrier::new(8));
    let mut workers: Vec<JoinHandle<()>> = Vec::new();
    for id in 0..8 {
        let barrier: Arc<Barrier> = barrier.clone();
        let home: PathBuf = fixture.root.path().to_path_buf();
        workers.push(thread::spawn(move || {
            let paths: Paths = Paths::new(BTreeMap::from([(
                OsString::from("HOME"),
                home.clone().into_os_string(),
            )]));
            let effective: Effective = Effective::load(&paths, &home).unwrap();
            let server: ripmcp::config::Server = serde_json::from_value(local("pkg")).unwrap();
            barrier.wait();
            effective
                .mutate(&paths, WriteScope::User, &id.to_string(), |configuration| {
                    configuration.register(id.to_string(), server)
                })
                .unwrap();
        }));
    }
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(fixture.load(fixture.root.path()).reports().len(), 8);
}

#[test]
fn store_lock_is_shared_across_processes() {
    let fixture: Fixture = Fixture::new();
    let directory: PathBuf = fixture.paths.directory(Location::State).unwrap();
    let mut children: Vec<std::process::Child> = Vec::new();
    for _ in 0..4 {
        children.push(
            Command::new(std::env::current_exe().unwrap())
                .env_clear()
                .env("RIPMCP_TEST_WRITER_DIRECTORY", &directory)
                .args(["--exact", "storage::writer_subprocess"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        );
    }
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }
    let store: Store = Store::new(directory, "processes.json", true).unwrap();
    let count: u32 = serde_json::from_slice(&store.read().unwrap().unwrap()).unwrap();
    assert_eq!(count, 80);
}

#[test]
fn writer_subprocess() {
    let Some(directory): Option<OsString> = std::env::var_os("RIPMCP_TEST_WRITER_DIRECTORY") else {
        return;
    };
    let store: Store = Store::new(PathBuf::from(directory), "processes.json", true).unwrap();
    for _ in 0..20 {
        store
            .update_json::<u32, _>(|count| {
                *count += 1;
                Ok(())
            })
            .unwrap();
    }
}

#[test]
fn lock_wait_respects_the_operation_deadline_without_running_the_mutation() {
    let fixture: Fixture = Fixture::new();
    let directory: PathBuf = fixture.paths.directory(Location::State).unwrap();
    let store: Store = Store::new(directory.clone(), "state.json", true).unwrap();
    store.update(|_| Ok((b"old".to_vec(), ()))).unwrap();
    let lock: std::fs::File = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(directory.join(".state.json.lock"))
        .unwrap();
    lock.lock().unwrap();
    let deadline: ripmcp::deadline::Deadline =
        ripmcp::deadline::Deadline::new(std::time::Duration::from_millis(10));
    let result: Result<(), Error> =
        store.update_with_deadline(&deadline, |_| panic!("locked mutation must not run"));
    assert_eq!(result.unwrap_err().kind, ErrorKind::Timeout);
    assert_eq!(store.read().unwrap().unwrap(), b"old");
    drop(lock);
    store.update(|_| Ok((b"new".to_vec(), ()))).unwrap();
    assert_eq!(store.read().unwrap().unwrap(), b"new");
}
