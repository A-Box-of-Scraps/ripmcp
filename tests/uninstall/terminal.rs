use super::*;
use rustix::pty::{OpenptFlags, grantpt, ioctl_tiocgptpeer, openpt, unlockpt};
use std::{
    fs::File,
    io::{Read, Write},
    process::{Child, Stdio},
    time::{Duration, Instant},
};

struct Terminal {
    child: Option<Child>,
    master: File,
}

impl Terminal {
    fn spawn(fixture: &Fixture) -> Self {
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
        let child: Child = fixture
            .command(&["uninstall", "s", "--clean"])
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
            assert!(started.elapsed() < Duration::from_secs(5));
        }
        String::from_utf8(output).unwrap()
    }
    fn answer(mut self, answer: &[u8]) -> Output {
        self.master.write_all(answer).unwrap();
        let started: Instant = Instant::now();
        while self.child.as_mut().unwrap().try_wait().unwrap().is_none() {
            assert!(started.elapsed() < Duration::from_secs(5));
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
fn decline_changes_nothing_and_approval_rejects_changed_ownership() {
    let fixture: Fixture = Fixture::new();
    install(&fixture);
    let before: Vec<u8> = fs::read(fixture.config()).unwrap();
    let ownership: PathBuf = fixture
        .paths
        .directory(ripmcp::storage::Location::State)
        .unwrap()
        .join("ownership.json");
    let records: Vec<u8> = fs::read(&ownership).unwrap();
    let mut terminal: Terminal = Terminal::spawn(&fixture);
    assert!(terminal.preview().contains("stop_selected_owned_process"));
    assert_eq!(terminal.answer(b"\n").status.code(), Some(130));
    assert_eq!(fs::read(fixture.config()).unwrap(), before);
    assert_eq!(fs::read(&ownership).unwrap(), records);
    assert_eq!(fixture.ok(&["start", "s"])["actions"][0]["reused"], true);
    let mut terminal: Terminal = Terminal::spawn(&fixture);
    let _: String = terminal.preview();
    let path: PathBuf = fixture.root.path().join("new");
    fs::write(&path, "new owned target outside approval").unwrap();
    track(&fixture, &path);
    assert_eq!(terminal.answer(b"y\n").status.code(), Some(3));
    assert_eq!(fs::read(fixture.config()).unwrap(), before);
    assert!(path.exists());
}
