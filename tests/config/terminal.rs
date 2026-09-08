use super::*;
use rustix::pty::{OpenptFlags, grantpt, ioctl_tiocgptpeer, openpt, unlockpt};
use std::fs::File;
use std::io::{Read, Write};
use std::process::Child;
use std::time::{Duration, Instant};

struct Terminal {
    child: Option<Child>,
    master: File,
}
impl Terminal {
    fn spawn(fixture: &Fixture, root: &Path) -> Self {
        let master: File = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC)
            .map(File::from)
            .unwrap();
        rustix::fs::fcntl_setfl(&master, rustix::fs::OFlags::NONBLOCK).unwrap();
        grantpt(&master).unwrap();
        unlockpt(&master).unwrap();
        let slave: File = ioctl_tiocgptpeer(
            &master,
            OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC,
        )
        .map(File::from)
        .unwrap();
        let child: Child = Command::new(env!("CARGO_BIN_EXE_ripmcp"))
            .env_clear()
            .env("HOME", fixture.root.path())
            .current_dir(root)
            .arg("trust")
            .stdin(slave.try_clone().unwrap())
            .stderr(slave)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        Self {
            child: Some(child),
            master,
        }
    }
    fn preview(&mut self) -> String {
        let started: Instant = Instant::now();
        let mut output: Vec<u8> = Vec::new();
        while !output.ends_with(b"[y/N] ") {
            let mut buffer: [u8; 8192] = [0; 8192];
            match self.master.read(&mut buffer) {
                Ok(count) => output.extend_from_slice(&buffer[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(1))
                }
                Err(error) => panic!("terminal read: {error}"),
            }
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "trust prompt did not arrive"
            );
        }
        String::from_utf8(output).unwrap()
    }
    fn answer(mut self, answer: &[u8]) -> Output {
        self.master.write_all(answer).unwrap();
        let started: Instant = Instant::now();
        while self.child.as_mut().unwrap().try_wait().unwrap().is_none() {
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "trust did not finish"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        self.child.take().unwrap().wait_with_output().unwrap()
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _: std::io::Result<()> = child.kill();
            let _: std::io::Result<std::process::ExitStatus> = child.wait();
        }
    }
}

#[test]
fn terminal_trust_previews_before_writes_and_confirms_exact_snapshot() {
    let fixture: Fixture = Fixture::new();
    let root: PathBuf = fixture.project(
        "project",
        &config(remote("https://example.test/secret?token=secret")),
    );
    let project: Project = Project::discover(&root).unwrap().unwrap();
    let mut terminal: Terminal = Terminal::spawn(&fixture, &root);
    let preview: String = terminal.preview();
    assert!(preview.contains("https://example.test"));
    assert!(!preview.contains("token=secret"));
    assert!(preview.contains(project.digest()));
    assert!(!fixture.root.path().join(".local").exists());
    fixture.project("project", &config(remote("https://changed.test/mcp")));
    let output: Output = terminal.answer(b"y\n");
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["trust"]["configuration_digest"], project.digest());
    assert_eq!(report["trust"]["approved"], true);
    assert!(
        TrustStore::new(&fixture.paths)
            .unwrap()
            .is_approved(&project)
            .unwrap()
    );
    assert!(fixture.load(&root).authorize("s").is_err());
}

#[test]
fn terminal_trust_defaults_to_no_and_can_approve_current_configuration() {
    let fixture: Fixture = Fixture::new();
    let root: PathBuf = fixture.project("project", &config(local("pkg")));
    let mut terminal: Terminal = Terminal::spawn(&fixture, &root);
    let _: String = terminal.preview();
    assert_eq!(terminal.answer(b"\n").status.code(), Some(3));
    assert!(!fixture.root.path().join(".local").exists());
    let mut terminal: Terminal = Terminal::spawn(&fixture, &root);
    let _: String = terminal.preview();
    assert!(terminal.answer(b"Y\n").status.success());
    assert!(fixture.load(&root).authorize("s").is_ok());
}
